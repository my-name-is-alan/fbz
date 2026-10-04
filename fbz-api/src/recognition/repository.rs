//! 识别词规则的持久化（design §3 规则配置层）。
//!
//! 提供 admin CRUD + 「加载并编译为内存 [`RuleSet`]」。扫描层在跑识别前按库
//! （全局规则 + 该库规则）加载编译一次，复用 `metadata` 同款「DB 合并 + 下次 job 重读」
//! 热更新模式：规则变更不影响进行中的 job，下个 job 重新编译。

use sqlx::{Row, postgres::PgRow};

use crate::db::DbPool;
use crate::recognition::rules::{RecognitionWord, RuleKind, RuleSet};

/// 一条识别词规则的完整记录（对外 DTO / admin 列表用）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecognitionWordRecord {
    pub id: String,
    pub kind: String,
    pub pattern: String,
    pub replacement: Option<String>,
    pub anchor_after: Option<String>,
    pub offset_expr: Option<String>,
    pub is_regex: bool,
    pub enabled: bool,
    /// 绑定的库 public_id；None = 全局规则。
    pub library_id: Option<String>,
    pub priority: i32,
    pub note: Option<String>,
}

/// 新增识别词的输入（admin 录入解析后）。
#[derive(Clone, Debug)]
pub struct CreateRecognitionWordInput {
    pub kind: RuleKind,
    pub pattern: String,
    pub replacement: Option<String>,
    pub anchor_after: Option<String>,
    pub offset_expr: Option<String>,
    pub is_regex: bool,
    pub enabled: bool,
    /// 库 public_id；None = 全局。
    pub library_public_id: Option<String>,
    pub priority: i32,
    pub note: Option<String>,
}

#[derive(Clone)]
pub struct RecognitionRepository {
    pool: DbPool,
}

impl RecognitionRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// 加载并编译某库适用的规则集（全局 + 该库），供识别管线使用。
    /// 坏正则在 [`RuleSet::compile`] 内跳过，不阻断（design §3 不变量）。
    /// `library_public_id` 为 None 时只加载全局规则。
    pub async fn load_ruleset_for_library(
        &self,
        library_public_id: Option<&str>,
    ) -> Result<(RuleSet, Vec<String>), sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select w.public_id::text as id,
                   w.kind,
                   w.pattern,
                   w.replacement,
                   w.anchor_after,
                   w.offset_expr,
                   w.is_regex,
                   w.priority
            from recognition_words w
            left join libraries l on l.id = w.library_id
            where w.enabled = true
              and (
                  w.library_id is null
                  or l.public_id = case
                      when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                      then $1::uuid
                      else null::uuid
                  end
              )
            order by w.priority asc, w.id asc
            "#,
        )
        .bind(library_public_id)
        .fetch_all(&self.pool)
        .await?;

        let mut words = Vec::new();
        for row in rows {
            let kind_str: String = row.try_get("kind")?;
            let Some(kind) = RuleKind::parse(&kind_str) else {
                continue; // 未知 kind（不该发生，CHECK 守卫）：跳过。
            };
            words.push(RecognitionWord {
                id: row.try_get("id")?,
                kind,
                pattern: row.try_get("pattern")?,
                replacement: row.try_get("replacement")?,
                anchor_after: row.try_get("anchor_after")?,
                offset_expr: row.try_get("offset_expr")?,
                is_regex: row.try_get("is_regex")?,
                priority: row.try_get("priority")?,
            });
        }
        Ok(RuleSet::compile(words))
    }

    /// 列出识别词规则（全局 + 可选按库过滤），priority 升序。
    pub async fn list_words(
        &self,
        library_public_id: Option<&str>,
    ) -> Result<Vec<RecognitionWordRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select w.public_id::text as id,
                   w.kind,
                   w.pattern,
                   w.replacement,
                   w.anchor_after,
                   w.offset_expr,
                   w.is_regex,
                   w.enabled,
                   lib.public_id::text as library_id,
                   w.priority,
                   w.note
            from recognition_words w
            left join libraries lib on lib.id = w.library_id
            where $1::text is null
               or lib.public_id = case
                   when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                   then $1::uuid
                   else null::uuid
               end
            order by w.priority asc, w.id asc
            "#,
        )
        .bind(library_public_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(RecognitionWordRecord::from_row)
            .collect()
    }

    /// 新增一条识别词；library_public_id 给定时解析为内部 id（无效库返回 None）。
    pub async fn create_word(
        &self,
        input: CreateRecognitionWordInput,
    ) -> Result<Option<RecognitionWordRecord>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            with target_library as (
                select case
                    when $8::text is null then null::bigint
                    else (
                        select id from libraries
                        where public_id = case
                            when $8::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                            then $8::uuid
                            else null::uuid
                        end
                    )
                end as library_id,
                -- 标记：给了 library 但解析不到 → 调用方应 404。
                ($8::text is not null) as library_requested
            )
            insert into recognition_words (
                kind, pattern, replacement, anchor_after, offset_expr,
                is_regex, enabled, library_id, priority, note
            )
            select $1, $2, $3, $4, $5, $6, $7, tl.library_id, $9, $10
            from target_library tl
            where not (tl.library_requested and tl.library_id is null)
            returning
                public_id::text as id, kind, pattern, replacement, anchor_after,
                offset_expr, is_regex, enabled,
                (select public_id::text from libraries where id = recognition_words.library_id) as library_id,
                priority, note
            "#,
        )
        .bind(input.kind.as_str())
        .bind(&input.pattern)
        .bind(input.replacement.as_deref())
        .bind(input.anchor_after.as_deref())
        .bind(input.offset_expr.as_deref())
        .bind(input.is_regex)
        .bind(input.enabled)
        .bind(input.library_public_id.as_deref())
        .bind(input.priority)
        .bind(input.note.as_deref())
        .fetch_optional(&self.pool)
        .await?;
        row.map(RecognitionWordRecord::from_row).transpose()
    }

    /// 删除一条识别词（按 public_id）。返回是否删到。
    pub async fn delete_word(&self, public_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            delete from recognition_words
            where public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            "#,
        )
        .bind(public_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

impl RecognitionWordRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            kind: row.try_get("kind")?,
            pattern: row.try_get("pattern")?,
            replacement: row.try_get("replacement")?,
            anchor_after: row.try_get("anchor_after")?,
            offset_expr: row.try_get("offset_expr")?,
            is_regex: row.try_get("is_regex")?,
            enabled: row.try_get("enabled")?,
            library_id: row.try_get("library_id")?,
            priority: row.try_get("priority")?,
            note: row.try_get("note")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_types::LibraryType;
    use crate::recognition::recognize;
    use crate::recognition::types::RecognitionInput;

    // Live-DB smoke: validates the recognition_words CRUD + RuleSet compilation
    // against the real migrated schema (migration 0082), and that a created
    // offset rule actually changes recognition output, then cleans up.
    //   cargo test -- --ignored recognition_words_crud_and_ruleset_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn recognition_words_crud_and_ruleset_against_live_schema() {
        use crate::recognition::rules::RuleKind;
        use sqlx::postgres::PgPoolOptions;
        use std::time::{SystemTime, UNIX_EPOCH};

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let repo = RecognitionRepository::new(pool.clone());
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let note = format!("smoke-{nonce}");

        // Create a global offset rule (-26 absolute→season episode).
        let created = repo
            .create_word(CreateRecognitionWordInput {
                kind: RuleKind::Offset,
                pattern: String::new(),
                replacement: None,
                anchor_after: Some(String::new()),
                offset_expr: Some("-26".to_owned()),
                is_regex: false,
                enabled: true,
                library_public_id: None,
                priority: 100,
                note: Some(note.clone()),
            })
            .await
            .expect("create offset word should execute")
            .expect("global rule must insert");
        assert_eq!(created.kind, "offset");

        // load_ruleset_for_library compiles it; recognition applies the offset.
        let (ruleset, skipped) = repo
            .load_ruleset_for_library(None)
            .await
            .expect("load ruleset should execute");
        assert!(skipped.is_empty(), "no bad regex expected");
        let input = RecognitionInput {
            file_stem: "Show.S02E38",
            extension: Some("mkv"),
            ancestors: &[],
        };
        let r = recognize(&input, LibraryType::TvShows, &ruleset);
        assert_eq!(
            r.episodes,
            vec![12],
            "live-loaded offset rule must correct absolute episode 38 → 12"
        );

        // list_words sees it.
        let listed = repo.list_words(None).await.expect("list should execute");
        assert!(listed.iter().any(|w| w.id == created.id));

        // delete removes it.
        let removed = repo
            .delete_word(&created.id)
            .await
            .expect("delete should execute");
        assert!(removed, "created rule must delete");
        let after = repo.list_words(None).await.expect("list after delete");
        assert!(!after.iter().any(|w| w.id == created.id));
    }
}
