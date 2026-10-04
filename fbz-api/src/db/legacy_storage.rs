//! Reconcile the two published branch histories before SQLx validates versions.
//! Unknown checksums are deliberately left untouched and fail normal SQLx validation.
use super::DbPool;
use sha2::{Digest, Sha384};
use sqlx::{Row, migrate::Migrator};
const OLD_CORE: [&str; 2] = [
    "e140ca11e9165a39ce553c1a9ff70b13cd99e4629b188191a9c45e821efc7c2d1cafe40dc34a59501b1b843ecc4666da",
    "dfb41cc664de41066b59625b28ef3d7d1a993f05464a878381d315629be44b69eb472bc3a8cdc1dd4f0fab24aff5e8a3",
];
fn historical_matches(bytes: &[u8], sql: &str) -> bool {
    let lf = sql.replace("\r\n", "\n");
    [lf.clone(), lf.replace('\n', "\r\n")]
        .iter()
        .any(|s| Sha384::digest(s.as_bytes()).as_slice() == bytes)
}
pub async fn reconcile(pool: &DbPool, migrator: &Migrator) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("select pg_advisory_xact_lock(732048129)")
        .execute(&mut *tx)
        .await?;
    let exists: bool = sqlx::query_scalar("select to_regclass('_sqlx_migrations') is not null")
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        return Ok(());
    }
    for (old, new) in [(77, 97), (78, 98), (79, 99)] {
        let Some(target) = migrator.iter().find(|m| m.version == new) else {
            continue;
        };
        if let Some(row) =
            sqlx::query("select checksum,success from _sqlx_migrations where version=$1")
                .bind(old)
                .fetch_optional(&mut *tx)
                .await?
        {
            let checksum: Vec<u8> = row.get("checksum");
            if row.get::<bool, _>("success") && historical_matches(&checksum, &target.sql) {
                sqlx::query("update _sqlx_migrations set version=$2,checksum=$3 where version=$1")
                    .bind(old)
                    .bind(new)
                    .bind(target.checksum.as_ref())
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    if let Some(row) = sqlx::query("select checksum,success from _sqlx_migrations where version=2")
        .fetch_optional(&mut *tx)
        .await?
    {
        let checksum: Vec<u8> = row.get("checksum");
        let hex = checksum
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if row.get::<bool, _>("success") && OLD_CORE.contains(&hex.as_str()) {
            // Apply exactly the schema changes made by the remote branch's edited 0002.
            sqlx::raw_sql("ALTER TABLE libraries DROP CONSTRAINT libraries_library_type_check; UPDATE libraries SET library_type='tvshows' WHERE library_type='tv'; ALTER TABLE libraries ADD CONSTRAINT libraries_library_type_check CHECK(library_type IN ('movies','tvshows','music','homevideos','mixed','livetv')); ALTER TABLE media_items DROP CONSTRAINT media_items_item_type_check; ALTER TABLE media_items ADD CONSTRAINT media_items_item_type_check CHECK(item_type IN ('folder','movie','series','season','episode','artist','album','track','collection','photo','video','tvchannel','program','recording'));").execute(&mut *tx).await?;
            let target = migrator
                .iter()
                .find(|m| m.version == 2)
                .expect("core migration");
            sqlx::query("update _sqlx_migrations set checksum=$1 where version=2")
                .bind(target.checksum.as_ref())
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_historical_sql_is_accepted() {
        let sql = "create table a();\n";
        let checksum = Sha384::digest(sql.as_bytes());
        assert!(historical_matches(&checksum, sql));
        assert!(historical_matches(&checksum, "create table a();\r\n"));
        assert!(!historical_matches(&checksum, "create table b();\n"));
    }
}

#[cfg(test)]
mod database_tests {
    #[tokio::test]
    #[ignore = "requires disposable FBZ_MIGRATION_TEST_URL"]
    async fn upgrade_preserves_storage_and_is_idempotent() {
        let url =
            std::env::var("FBZ_MIGRATION_TEST_URL").expect("explicit disposable database URL");
        assert!(
            url.contains("fbz_merge_test"),
            "test must target the disposable merge database"
        );
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        let before: Vec<(String, i64)> =
            sqlx::query_as("select id::text,library_id from storage_mounts order by id")
                .fetch_all(&pool)
                .await
                .unwrap();
        super::super::migrate(&pool).await.unwrap();
        super::super::migrate(&pool).await.unwrap();
        let after: Vec<(String, i64)> =
            sqlx::query_as("select id::text,library_id from storage_mounts order by id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(before, after);
        let records:i64=sqlx::query_scalar("select count(*) from _sqlx_migrations where version in (77,78,79,97,98,99) and success").fetch_one(&pool).await.unwrap();
        assert_eq!(records, 6);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("select count(*) from libraries where library_type='tv'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }
}
