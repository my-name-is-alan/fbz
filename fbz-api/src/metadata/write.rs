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
        let person_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into people (
                name,
                name_normalized
            )
            values ($1, $2)
            on conflict (name_normalized) do update
                set name = people.name,
                    updated_at = now()
            returning id
            "#,
        )
        .bind(person.name())
        .bind(person.name_normalized())
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
