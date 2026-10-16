//! 用户访问策略（JF UserPolicy 的文件夹/标签部分）原样存储与回显。
//!
//! 隔离说明：本模块只读写 `user_access_policies`（迁移 0110）。真正的权限判定仍由
//! `library_permissions.can_view`（库级，`routes/users.rs::apply_folder_access` 推导写入）和
//! SQL 函数 `fbz_item_visible_to_user`（条目级：MaxParentalRating + BlockedTags）完成。

use serde_json::Value;

use crate::db::DbPool;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RawAccessPolicy {
    pub enable_all_folders: bool,
    pub enabled_folders: Vec<String>,
    pub blocked_folders: Vec<String>,
    pub blocked_tags: Vec<String>,
}

/// 请求体里出现的字段（None = 未携带，保持已存值）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccessPolicyPatch {
    pub enable_all_folders: Option<bool>,
    pub enabled_folders: Option<Vec<String>>,
    pub blocked_folders: Option<Vec<String>>,
    pub blocked_tags: Option<Vec<String>>,
}

impl AccessPolicyPatch {
    pub fn is_empty(&self) -> bool {
        self.enable_all_folders.is_none()
            && self.enabled_folders.is_none()
            && self.blocked_folders.is_none()
            && self.blocked_tags.is_none()
    }
}

fn string_list(policy: &Value, keys: &[&str]) -> Option<Vec<String>> {
    let value = keys.iter().find_map(|k| policy.get(*k))?;
    let mut out: Vec<String> = Vec::new();
    for item in value.as_array().map(Vec::as_slice).unwrap_or_default() {
        if let Some(s) = item.as_str().map(str::trim).filter(|s| !s.is_empty()) {
            if !out.iter().any(|x| x.eq_ignore_ascii_case(s)) {
                out.push(s.to_owned());
            }
        }
    }
    Some(out)
}

pub fn patch_from_policy(policy: &Value) -> AccessPolicyPatch {
    AccessPolicyPatch {
        enable_all_folders: ["EnableAllFolders", "enableAllFolders"].iter().find_map(|k| policy.get(*k)?.as_bool()),
        enabled_folders: string_list(policy, &["EnabledFolders", "enabledFolders"]),
        blocked_folders: string_list(policy, &["BlockedMediaFolders", "BlockedFolders", "blockedMediaFolders", "blockedFolders"]),
        blocked_tags: string_list(policy, &["BlockedTags", "blockedTags"]),
    }
}

/// 合并：未携带字段保留 `base`（无存档时 base 由调用方给出当前推导值）。
pub fn merge(base: &RawAccessPolicy, patch: &AccessPolicyPatch) -> RawAccessPolicy {
    RawAccessPolicy {
        enable_all_folders: patch.enable_all_folders.unwrap_or(base.enable_all_folders),
        enabled_folders: patch.enabled_folders.clone().unwrap_or_else(|| base.enabled_folders.clone()),
        blocked_folders: patch.blocked_folders.clone().unwrap_or_else(|| base.blocked_folders.clone()),
        blocked_tags: patch.blocked_tags.clone().unwrap_or_else(|| base.blocked_tags.clone()),
    }
}

pub async fn load(pool: &DbPool, user_public_id: &str) -> Result<Option<RawAccessPolicy>, sqlx::Error> {
    let row: Option<(bool, Vec<String>, Vec<String>, Vec<String>)> = sqlx::query_as(
        "select p.enable_all_folders, p.enabled_folders, p.blocked_folders, p.blocked_tags
         from user_access_policies p join users u on u.id = p.user_id
         where u.public_id::text = lower($1)",
    )
    .bind(user_public_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(enable_all_folders, enabled_folders, blocked_folders, blocked_tags)| RawAccessPolicy {
        enable_all_folders,
        enabled_folders,
        blocked_folders,
        blocked_tags,
    }))
}

pub async fn save(pool: &DbPool, user_public_id: &str, policy: &RawAccessPolicy) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into user_access_policies (user_id, enable_all_folders, enabled_folders, blocked_folders, blocked_tags, updated_at)
         select id, $2, $3, $4, $5, now() from users where public_id::text = lower($1)
         on conflict (user_id) do update set enable_all_folders = excluded.enable_all_folders,
             enabled_folders = excluded.enabled_folders, blocked_folders = excluded.blocked_folders,
             blocked_tags = excluded.blocked_tags, updated_at = now()",
    )
    .bind(user_public_id)
    .bind(policy.enable_all_folders)
    .bind(&policy.enabled_folders)
    .bind(&policy.blocked_folders)
    .bind(&policy.blocked_tags)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn patch_reads_jf_and_emby_keys_and_merge_keeps_absent_fields() {
        let patch = patch_from_policy(&json!({
            "EnableAllFolders": false,
            "BlockedMediaFolders": ["a", " A ", ""],
            "BlockedTags": ["Horror"]
        }));
        assert_eq!(patch.enable_all_folders, Some(false));
        assert_eq!(patch.enabled_folders, None);
        assert_eq!(patch.blocked_folders, Some(vec!["a".to_owned()]));
        let base = RawAccessPolicy { enable_all_folders: true, enabled_folders: vec!["x".into()], ..Default::default() };
        let merged = merge(&base, &patch);
        assert!(!merged.enable_all_folders);
        assert_eq!(merged.enabled_folders, ["x"]);
        assert_eq!(merged.blocked_tags, ["Horror"]);
        assert!(patch_from_policy(&json!({"IsDisabled": false})).is_empty());
    }
}
