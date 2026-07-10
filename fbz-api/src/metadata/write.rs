use sqlx::{Postgres, Transaction};

use crate::metadata::provider::{MetadataNamedValue, MetadataPerson};

pub trait NamedMetadata {
    fn name(&self) -> &str;
    fn name_normalized(&self) -> &str;
}

pub trait PersonMetadata {
    fn name(&self) -> &str;
    fn name_normalized(&self) -> &str;
    fn role_type(&self) -> &str;
    fn role_name(&self) -> &str;
    fn sort_order(&self) -> i32;
    fn profile_image_url(&self) -> Option<&str>;
}

impl NamedMetadata for MetadataNamedValue {
    fn name(&self) -> &str {
        &self.name
    }

    fn name_normalized(&self) -> &str {
        &self.name_normalized
    }
}

impl PersonMetadata for MetadataPerson {
    fn name(&self) -> &str {
        &self.name
    }

    fn name_normalized(&self) -> &str {
        &self.name_normalized
    }

    fn role_type(&self) -> &str {
        &self.role_type
    }

    fn role_name(&self) -> &str {
        &self.role_name
    }

    fn sort_order(&self) -> i32 {
        self.sort_order
    }

    fn profile_image_url(&self) -> Option<&str> {
        self.profile_image_url.as_deref()
    }
}

pub async fn replace_item_genres<T>(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    genres: &[T],
) -> Result<(), sqlx::Error>
where
    T: NamedMetadata,
{
    sqlx::query(
        r#"
        delete from media_item_genres
        where media_item_id = $1
        "#,
    )
    .bind(media_item_id)
    .execute(&mut **tx)
    .await?;

    for genre in genres {
        let genre_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into genres (name, name_normalized)
            values ($1, $2)
            on conflict (name_normalized) do update
                set name = genres.name
            returning id
            "#,
        )
        .bind(genre.name())
        .bind(genre.name_normalized())
        .fetch_one(&mut **tx)
        .await?;

        sqlx::query(
            r#"
            insert into media_item_genres (media_item_id, genre_id)
            values ($1, $2)
            on conflict do nothing
            "#,
        )
        .bind(media_item_id)
        .bind(genre_id)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

pub async fn replace_item_studios<T>(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    studios: &[T],
) -> Result<(), sqlx::Error>
where
    T: NamedMetadata,
{
    sqlx::query(
        r#"
        delete from media_item_studios
        where media_item_id = $1
        "#,
    )
    .bind(media_item_id)
    .execute(&mut **tx)
    .await?;

    for studio in studios {
        let studio_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into studios (name, name_normalized)
            values ($1, $2)
            on conflict (name_normalized) do update
                set name = studios.name,
                    updated_at = now()
            returning id
            "#,
        )
        .bind(studio.name())
        .bind(studio.name_normalized())
        .fetch_one(&mut **tx)
        .await?;

        sqlx::query(
            r#"
            insert into media_item_studios (media_item_id, studio_id)
            values ($1, $2)
            on conflict do nothing
            "#,
        )
        .bind(media_item_id)
        .bind(studio_id)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

pub async fn replace_item_people<T>(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    people: &[T],
) -> Result<(), sqlx::Error>
where
    T: PersonMetadata,
{
    sqlx::query(
        r#"
        delete from media_item_people
        where media_item_id = $1
        "#,
    )
    .bind(media_item_id)
    .execute(&mut **tx)
    .await?;

    for person in people {
        let pinyin = crate::text::pinyin::pinyin_keys(person.name());
        let person_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into people (
                name,
                name_normalized,
                pinyin_full,
                pinyin_initials
            )
            values ($1, $2, $3, $4)
            on conflict (name_normalized) do update
                set name = people.name,
                    pinyin_full = coalesce(excluded.pinyin_full, people.pinyin_full),
                    pinyin_initials = coalesce(excluded.pinyin_initials, people.pinyin_initials),
                    updated_at = now()
            returning id
            "#,
        )
        .bind(person.name())
        .bind(person.name_normalized())
        .bind(pinyin.as_ref().map(|keys| keys.full.as_str()))
        .bind(pinyin.as_ref().map(|keys| keys.initials.as_str()))
        .fetch_one(&mut **tx)
        .await?;

        sqlx::query(
            r#"
            insert into media_item_people (
                media_item_id,
                person_id,
                role_type,
                role_name,
                sort_order
            )
            values ($1, $2, $3, $4, $5)
            on conflict do nothing
            "#,
        )
        .bind(media_item_id)
        .bind(person_id)
        .bind(person.role_type())
        .bind(person.role_name())
        .bind(person.sort_order())
        .execute(&mut **tx)
        .await?;

        // 人物头像：只存 TMDB CDN 的 remote_url（不下载字节，跟海报同机制）。
        // 无 unique 约束，先删旧 primary 再插；为 None 时跳过以免抹掉已有图。
        if let Some(profile_url) = person.profile_image_url() {
            sqlx::query(
                r#"
                delete from artwork
                where person_id = $1 and artwork_type = 'primary'
                "#,
            )
            .bind(person_id)
            .execute(&mut **tx)
            .await?;

            sqlx::query(
                r#"
                insert into artwork (
                    person_id,
                    artwork_type,
                    source,
                    remote_url,
                    is_primary
                )
                values ($1, 'primary', 'tmdb', $2, true)
                "#,
            )
            .bind(person_id)
            .bind(profile_url)
            .execute(&mut **tx)
            .await?;
        }
    }

    Ok(())
}

/// 替换 media_item 的播出/发行平台关联（networks）。仿 studios：实体表 upsert + 关联表重建。
pub async fn replace_item_networks<T>(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    networks: &[T],
) -> Result<(), sqlx::Error>
where
    T: NamedMetadata,
{
    sqlx::query("delete from media_item_networks where media_item_id = $1")
        .bind(media_item_id)
        .execute(&mut **tx)
        .await?;

    for network in networks {
        let network_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into networks (name, name_normalized)
            values ($1, $2)
            on conflict (name_normalized) do update
                set name = networks.name,
                    updated_at = now()
            returning id
            "#,
        )
        .bind(network.name())
        .bind(network.name_normalized())
        .fetch_one(&mut **tx)
        .await?;

        sqlx::query(
            r#"
            insert into media_item_networks (media_item_id, network_id)
            values ($1, $2)
            on conflict do nothing
            "#,
        )
        .bind(media_item_id)
        .bind(network_id)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

/// 主题曲 / 宣传片 / 预告等附属视频的字段访问（供 `replace_item_videos` 解耦 DTO）。
pub trait VideoMetadata {
    fn video_type(&self) -> &str;
    fn name(&self) -> Option<&str>;
    fn site(&self) -> Option<&str>;
    fn site_key(&self) -> Option<&str>;
    fn url(&self) -> Option<&str>;
    fn is_official(&self) -> bool;
    fn sort_order(&self) -> i32;
}

impl VideoMetadata for crate::metadata::provider::MetadataVideo {
    fn video_type(&self) -> &str {
        &self.video_type
    }
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    fn site(&self) -> Option<&str> {
        self.site.as_deref()
    }
    fn site_key(&self) -> Option<&str> {
        self.site_key.as_deref()
    }
    fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }
    fn is_official(&self) -> bool {
        self.is_official
    }
    fn sort_order(&self) -> i32 {
        self.sort_order
    }
}

/// 替换 media_item 的附属视频（主题曲/宣传片/预告）。先全删再插，按 (site, site_key) 去重。
pub async fn replace_item_videos<T>(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: i64,
    videos: &[T],
) -> Result<(), sqlx::Error>
where
    T: VideoMetadata,
{
    sqlx::query("delete from media_videos where media_item_id = $1")
        .bind(media_item_id)
        .execute(&mut **tx)
        .await?;

    for video in videos {
        sqlx::query(
            r#"
            insert into media_videos (
                media_item_id, video_type, name, site, site_key, url, is_official, sort_order
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8)
            on conflict (media_item_id, site, site_key) do nothing
            "#,
        )
        .bind(media_item_id)
        .bind(video.video_type())
        .bind(video.name())
        .bind(video.site())
        .bind(video.site_key())
        .bind(video.url())
        .bind(video.is_official())
        .bind(video.sort_order())
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct NamedFixture {
        name: String,
        name_normalized: String,
    }

    impl NamedMetadata for NamedFixture {
        fn name(&self) -> &str {
            &self.name
        }

        fn name_normalized(&self) -> &str {
            &self.name_normalized
        }
    }

    #[derive(Clone)]
    struct PersonFixture {
        name: String,
        name_normalized: String,
        role_type: String,
        role_name: String,
        sort_order: i32,
    }

    impl PersonMetadata for PersonFixture {
        fn name(&self) -> &str {
            &self.name
        }

        fn name_normalized(&self) -> &str {
            &self.name_normalized
        }

        fn role_type(&self) -> &str {
            &self.role_type
        }

        fn role_name(&self) -> &str {
            &self.role_name
        }

        fn sort_order(&self) -> i32 {
            self.sort_order
        }

        fn profile_image_url(&self) -> Option<&str> {
            None
        }
    }

    #[test]
    fn named_metadata_trait_preserves_name_boundaries() {
        let value = NamedFixture {
            name: "Studio A".to_owned(),
            name_normalized: "studio a".to_owned(),
        };

        assert_eq!(value.name(), "Studio A");
        assert_eq!(value.name_normalized(), "studio a");
    }

    #[test]
    fn person_metadata_trait_preserves_relationship_fields() {
        let value = PersonFixture {
            name: "Jane Doe".to_owned(),
            name_normalized: "jane doe".to_owned(),
            role_type: "actor".to_owned(),
            role_name: "Lead".to_owned(),
            sort_order: 7,
        };

        assert_eq!(value.name(), "Jane Doe");
        assert_eq!(value.name_normalized(), "jane doe");
        assert_eq!(value.role_type(), "actor");
        assert_eq!(value.role_name(), "Lead");
        assert_eq!(value.sort_order(), 7);
    }
}
