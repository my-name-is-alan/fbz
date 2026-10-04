//! Bounded presentation enrichment for the first-party media client.
use crate::{
    compat::emby::routes::access::authenticate_request_user, error::AppError, state::AppState,
};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, Uri},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::Value;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/media/presentation", post(presentation))
        .route("/api/media/library-counts", get(library_counts))
}
#[derive(Deserialize)]
struct Request {
    ids: Vec<String>,
}
fn valid_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
async fn presentation(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Json(input): Json<Request>,
) -> Result<Json<Vec<Value>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if input.ids.len() > 100 || input.ids.iter().any(|id| !valid_id(id)) {
        return Err(AppError::unprocessable("invalid media identifiers"));
    }
    let pool = state
        .database()
        .ok_or_else(|| AppError::internal("database unavailable"))?;
    let records=sqlx::query_scalar::<_,Value>(r#"
      select jsonb_build_object(
        'Id',mi.public_id,'Overview',mi.overview,'CommunityRating',mi.community_rating,
        'Genres',coalesce((select jsonb_agg(g.name order by g.name) from media_item_genres mg join genres g on g.id=mg.genre_id where mg.media_item_id=mi.id),'[]'::jsonb),
        'IndexNumber',coalesce(mi.episode_number,mi.index_number),'ParentIndexNumber',coalesce(mi.season_number,mi.parent_index_number),
        'SeriesId',case when p.item_type='series' then p.public_id when gp.item_type='series' then gp.public_id else null end,
        'SeriesName',case when p.item_type='series' then p.title when gp.item_type='series' then gp.title else null end,
        'PrimaryImageItemId',poster.owner_id,'BackdropImageItemId',backdrop.owner_id,
        'ImageTags',case when poster.id is null then '{}'::jsonb else jsonb_build_object('Primary',poster.id::text) end,
        'BackdropImageTags',case when backdrop.id is null then '[]'::jsonb else jsonb_build_array(backdrop.id::text) end
      )
      from media_items mi join libraries l on l.id=mi.library_id
      join library_permissions lp on lp.library_id=l.id and lp.user_id=$1 and lp.can_view
      left join media_items p on p.id=mi.parent_id and p.library_id=mi.library_id
      left join media_items gp on gp.id=p.parent_id and gp.library_id=mi.library_id
      left join lateral (
        select a.id,owner.public_id as owner_id from artwork a join media_items owner on owner.id=a.media_item_id
        where a.media_item_id in(mi.id,p.id,gp.id) and a.artwork_type in('poster','primary')
        order by (a.media_item_id=mi.id) desc,(a.media_item_id=p.id) desc,a.is_primary desc,a.id limit 1
      ) poster on true
      left join lateral (
        select a.id,owner.public_id as owner_id from artwork a join media_items owner on owner.id=a.media_item_id
        where a.media_item_id in(mi.id,p.id,gp.id) and a.artwork_type='backdrop'
        order by (a.media_item_id=mi.id) desc,(a.media_item_id=p.id) desc,a.is_primary desc,a.id limit 1
      ) backdrop on true
      where not mi.is_deleted and not l.is_hidden and mi.public_id=any($2::text[]::uuid[])
    "#).bind(user.id).bind(input.ids).fetch_all(pool).await.map_err(|_|AppError::internal("failed to read media presentation"))?;
    Ok(Json(records))
}
async fn library_counts(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<Value>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let pool = state
        .database()
        .ok_or_else(|| AppError::internal("database unavailable"))?;
    let records=sqlx::query_scalar::<_,Value>("select jsonb_build_object('Id',l.public_id,'Count',count(mi.id)) from libraries l join library_permissions lp on lp.library_id=l.id and lp.user_id=$1 and lp.can_view left join media_items mi on mi.library_id=l.id and not mi.is_deleted and mi.item_type in('movie','series','track') where not l.is_hidden group by l.id")
        .bind(user.id).fetch_all(pool).await.map_err(|_|AppError::internal("failed to count media libraries"))?;
    Ok(Json(records))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_are_bounded_uuid_text() {
        assert!(valid_id("245753df-6dd9-498d-a03a-f6c494dd0c4e"));
        assert!(!valid_id("../secret"));
        assert!(!valid_id("245753df-6dd9-498d-a03a-f6c494dd0c4g"));
    }
}
