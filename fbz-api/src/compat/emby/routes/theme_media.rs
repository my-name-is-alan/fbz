use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    auth::service::AuthenticatedUser,
    compat::emby::dto::{AllThemeMediaResultDto, BaseItemDto, BaseItemSource, ThemeMediaResultDto},
    error::AppError,
    library::repository::{ItemVideoRecord, LibraryRepository, LocalExtraItemRecord},
    media::extras::ExtraType,
    state::AppState,
};

use super::{
    access::authenticate_request_user,
    items::media_item_to_base_item_with_extra_type,
};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ThemeMediaQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "inheritFromParent", alias = "inherit_from_parent")]
    pub inherit_from_parent: Option<bool>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
}

pub async fn theme_media(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<ThemeMediaQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<AllThemeMediaResultDto>, AppError> {
    let user = authenticate_theme_user(&state, &query, &headers, &uri).await?;
    let (songs, videos, soundtrack) = load_theme_buckets(&state, user, &item_id, &query).await?;
    Ok(Json(AllThemeMediaResultDto {
        theme_videos_result: videos,
        theme_songs_result: songs,
        soundtrack_songs_result: soundtrack,
    }))
}

pub async fn theme_songs(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<ThemeMediaQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ThemeMediaResultDto>, AppError> {
    let user = authenticate_theme_user(&state, &query, &headers, &uri).await?;
    let (songs, _, _) = load_theme_buckets(&state, user, &item_id, &query).await?;
    Ok(Json(songs))
}

pub async fn theme_videos(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<ThemeMediaQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ThemeMediaResultDto>, AppError> {
    let user = authenticate_theme_user(&state, &query, &headers, &uri).await?;
    let (_, videos, _) = load_theme_buckets(&state, user, &item_id, &query).await?;
    Ok(Json(videos))
}

async fn authenticate_theme_user(
    state: &AppState,
    query: &ThemeMediaQuery,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<AuthenticatedUser, AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;
    if let Some(query_user_id) = query.user_id.as_deref()
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }
    Ok(user)
}

async fn load_theme_buckets(
    state: &AppState,
    user: AuthenticatedUser,
    item_id: &str,
    query: &ThemeMediaQuery,
) -> Result<(ThemeMediaResultDto, ThemeMediaResultDto, ThemeMediaResultDto), AppError> {
    let _ = query.inherit_from_parent;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = LibraryRepository::new(database.clone());

    let Some(local) = repository
        .list_local_extra_items(user.id, item_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to list local theme media: {err}")))?
    else {
        return Err(AppError::not_found("item not found"));
    };

    let remote = repository
        .list_item_videos(item_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to list item videos: {err}")))?;

    let mut theme_songs: Vec<BaseItemDto> = Vec::new();
    let mut theme_videos: Vec<BaseItemDto> = Vec::new();

    for extra in local {
        match parse_extra_type(&extra.extra_type) {
            Some(ExtraType::ThemeSong) => {
                theme_songs.push(local_extra_to_base_item(&extra, ExtraType::ThemeSong));
            }
            Some(ExtraType::ThemeVideo) => {
                theme_videos.push(local_extra_to_base_item(&extra, ExtraType::ThemeVideo));
            }
            _ => {}
        }
    }

    for video in remote {
        match theme_bucket_for_video(&video.video_type) {
            ThemeBucket::Song => theme_songs.push(remote_video_to_base_item(&video, "Audio")),
            ThemeBucket::Video => theme_videos.push(remote_video_to_base_item(&video, "Video")),
            ThemeBucket::Skip => {}
        }
    }

    let start = query.start_index.unwrap_or(0) as usize;
    let limit = query.limit.map(|v| v as usize);

    Ok((
        slice_theme_result(item_id, theme_songs, start, limit),
        slice_theme_result(item_id, theme_videos, start, limit),
        ThemeMediaResultDto::empty(item_id.to_owned()),
    ))
}

enum ThemeBucket {
    Song,
    Video,
    Skip,
}

fn theme_bucket_for_video(video_type: &str) -> ThemeBucket {
    match video_type {
        "opening_theme" | "ending_theme" | "theme" => ThemeBucket::Song,
        // treat explicit theme_video if ever stored
        "theme_video" => ThemeBucket::Video,
        // Remote trailers stay on LocalTrailers / Trailers.
        _ => ThemeBucket::Skip,
    }
}

fn parse_extra_type(value: &str) -> Option<ExtraType> {
    ExtraType::ALL
        .into_iter()
        .find(|kind| kind.as_str() == value)
}

fn local_extra_to_base_item(extra: &LocalExtraItemRecord, kind: ExtraType) -> BaseItemDto {
    media_item_to_base_item_with_extra_type(extra.item.clone(), kind)
}

fn remote_video_to_base_item(video: &ItemVideoRecord, item_type: &str) -> BaseItemDto {
    let name = video
        .name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| video.url.clone())
        .unwrap_or_else(|| video.video_type.clone());
    let mut item = BaseItemDto::from(BaseItemSource {
        id: video.id.clone(),
        name,
        item_type: item_type.to_owned(),
        media_type: Some(if item_type == "Audio" {
            "Audio".to_owned()
        } else {
            "Video".to_owned()
        }),
        parent_id: None,
        is_folder: false,
        run_time_ticks: None,
        production_year: None,
    });
    item.overview = video.url.clone();
    item
}

fn slice_theme_result(
    owner_id: &str,
    items: Vec<BaseItemDto>,
    start: usize,
    limit: Option<usize>,
) -> ThemeMediaResultDto {
    let total = items.len() as u32;
    let sliced: Vec<BaseItemDto> = items
        .into_iter()
        .skip(start)
        .take(limit.unwrap_or(usize::MAX))
        .collect();
    ThemeMediaResultDto {
        owner_id: owner_id.to_owned(),
        items: sliced,
        total_record_count: total,
    }
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use http::Uri;
    use serde_json::json;

    use super::*;

    #[test]
    fn theme_media_query_accepts_common_emby_parameters() {
        let query = serde_json::from_value::<ThemeMediaQuery>(json!({
            "UserId": "user-1",
            "InheritFromParent": true,
            "StartIndex": 2,
            "Limit": 10,
            "Fields": "MediaSources"
        }))
        .unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.inherit_from_parent, Some(true));
        assert_eq!(query.start_index, Some(2));
        assert_eq!(query.limit, Some(10));
        assert_eq!(query.fields.as_deref(), Some("MediaSources"));
    }

    #[test]
    fn theme_media_query_accepts_lower_camel_client_fields() {
        let uri = "/Items/item-1/ThemeMedia?userId=user-1&inheritFromParent=true&startIndex=2&limit=10&fields=MediaSources"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<ThemeMediaQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.inherit_from_parent, Some(true));
        assert_eq!(query.start_index, Some(2));
        assert_eq!(query.limit, Some(10));
        assert_eq!(query.fields.as_deref(), Some("MediaSources"));
    }

    #[test]
    fn theme_bucket_maps_opening_theme_to_songs() {
        assert!(matches!(
            theme_bucket_for_video("opening_theme"),
            ThemeBucket::Song
        ));
        assert!(matches!(theme_bucket_for_video("trailer"), ThemeBucket::Skip));
    }

    #[test]
    fn extras_helpers_used_for_theme_surface() {
        assert!(crate::media::extras::is_theme_media(ExtraType::ThemeSong));
        assert_eq!(
            crate::media::extras::emby_extra_item_type(ExtraType::ThemeVideo),
            "Video"
        );
    }
}
