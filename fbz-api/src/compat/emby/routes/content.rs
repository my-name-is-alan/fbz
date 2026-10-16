use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::{Deserialize, Serialize};

use crate::{
    compat::emby::dto::{BaseItemDto, QueryResultDto},
    error::AppError,
    state::AppState,
    users::repository::UsersRepository,
};

use super::{
    access::authenticate_route_user,
    items::{self, ItemsQuery},
};

const DEFAULT_HOME_SECTION_ITEM_LIMIT: u32 = 12;
const MAX_CONTENT_SECTION_ID_LEN: usize = 64;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ContentSectionDto {
    pub id: String,
    pub name: String,
    pub section_type: String,
    pub view_type: String,
    pub scroll_direction: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_info: Option<String>,
    pub card_size_offset: i32,
    pub refresh_interval: i32,
    pub monitor: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ContentSectionItemsQuery {
    #[serde(alias = "parentId", alias = "parent_id")]
    pub parent_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "recursive")]
    pub recursive: Option<bool>,
    #[serde(alias = "includeItemTypes", alias = "include_item_types")]
    pub include_item_types: Option<String>,
    #[serde(alias = "mediaTypes", alias = "media_types")]
    pub media_types: Option<String>,
    #[serde(alias = "sortBy", alias = "sort_by")]
    pub sort_by: Option<String>,
    #[serde(alias = "sortOrder", alias = "sort_order")]
    pub sort_order: Option<String>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
    #[serde(alias = "enableImages", alias = "enable_images")]
    pub enable_images: Option<bool>,
    #[serde(alias = "imageTypeLimit", alias = "image_type_limit")]
    pub image_type_limit: Option<u32>,
    #[serde(alias = "enableImageTypes", alias = "enable_image_types")]
    pub enable_image_types: Option<String>,
    #[serde(alias = "filters")]
    pub filters: Option<String>,
    #[serde(alias = "isFavorite", alias = "is_favorite")]
    pub is_favorite: Option<bool>,
    #[serde(alias = "isPlayed", alias = "is_played")]
    pub is_played: Option<bool>,
}

pub async fn home_sections(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<ContentSectionDto>>, AppError> {
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    let defaults = default_home_sections();
    let Some(database) = state.database() else {
        return Ok(Json(defaults));
    };

    let repository = UsersRepository::new(database.clone());
    let preferred_ids = preferred_home_section_ids(&repository, user.id).await;
    Ok(Json(order_home_sections(defaults, preferred_ids)))
}

pub async fn section_items(
    State(state): State<AppState>,
    Path((user_id, section_id)): Path<(String, String)>,
    Query(query): Query<ContentSectionItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    let section_id = normalized_section_id(&section_id)?;
    let Some(items_query) = section_items_query(&section_id, query) else {
        return Ok(Json(QueryResultDto::new(Vec::new(), 0, 0)));
    };

    Ok(Json(
        items::list_items_for_authenticated_user(database.clone(), user, items_query).await?,
    ))
}

fn default_home_sections() -> Vec<ContentSectionDto> {
    vec![
        content_section(
            "latestmedia",
            "Latest media",
            "LatestMedia",
            Some("mixed".to_owned()),
        ),
        content_section(
            "resume",
            "Continue watching",
            "Resume",
            Some("mixed".to_owned()),
        ),
        content_section(
            "favorites",
            "Favorites",
            "Favorites",
            Some("mixed".to_owned()),
        ),
        content_section("movies", "Movies", "Library", Some("movies".to_owned())),
        content_section("series", "Series", "Library", Some("tvshows".to_owned())),
        content_section("music", "Music", "Library", Some("music".to_owned())),
    ]
}

fn content_section(
    id: &str,
    name: &str,
    section_type: &str,
    collection_type: Option<String>,
) -> ContentSectionDto {
    ContentSectionDto {
        id: id.to_owned(),
        name: name.to_owned(),
        section_type: section_type.to_owned(),
        view_type: "Items".to_owned(),
        scroll_direction: "Horizontal".to_owned(),
        collection_type,
        subtitle: None,
        text_info: None,
        card_size_offset: 0,
        refresh_interval: 0,
        monitor: false,
    }
}

fn section_items_query(section_id: &str, query: ContentSectionItemsQuery) -> Option<ItemsQuery> {
    let mut items_query = base_items_query(query);
    match section_id {
        "latest" | "latestmedia" | "recentlyadded" => {
            items_query.recursive = Some(true);
            items_query
                .sort_by
                .get_or_insert_with(|| "DateCreated".to_owned());
            items_query
                .sort_order
                .get_or_insert_with(|| "Descending".to_owned());
            Some(items_query)
        }
        "resume" | "continuewatching" | "continue-watching" => {
            items_query.recursive = Some(true);
            items_query.filters = Some(append_filter(items_query.filters, "IsResumable"));
            items_query
                .sort_by
                .get_or_insert_with(|| "DateCreated".to_owned());
            items_query
                .sort_order
                .get_or_insert_with(|| "Descending".to_owned());
            Some(items_query)
        }
        "favorites" | "favourites" => {
            items_query.recursive = Some(true);
            items_query.is_favorite.get_or_insert(true);
            Some(items_query)
        }
        "movies" => {
            items_query.recursive = Some(true);
            items_query
                .include_item_types
                .get_or_insert_with(|| "Movie".to_owned());
            Some(items_query)
        }
        "series" | "shows" | "tvshows" => {
            items_query.recursive = Some(true);
            items_query
                .include_item_types
                .get_or_insert_with(|| "Series".to_owned());
            Some(items_query)
        }
        "music" | "songs" => {
            items_query.recursive = Some(true);
            items_query
                .include_item_types
                .get_or_insert_with(|| "Audio".to_owned());
            items_query
                .media_types
                .get_or_insert_with(|| "Audio".to_owned());
            Some(items_query)
        }
        "albums" => {
            items_query.recursive = Some(true);
            items_query
                .include_item_types
                .get_or_insert_with(|| "MusicAlbum".to_owned());
            Some(items_query)
        }
        "playlists" => {
            items_query
                .include_item_types
                .get_or_insert_with(|| "Playlist".to_owned());
            Some(items_query)
        }
        "libraries" | "views" => Some(items_query),
        _ => None,
    }
}

fn base_items_query(query: ContentSectionItemsQuery) -> ItemsQuery {
    ItemsQuery {
        parent_id: query.parent_id,
        start_index: query.start_index,
        limit: Some(query.limit.unwrap_or(DEFAULT_HOME_SECTION_ITEM_LIMIT)),
        recursive: query.recursive,
        include_item_types: query.include_item_types,
        media_types: query.media_types,
        sort_by: query.sort_by,
        sort_order: query.sort_order,
        fields: query.fields,
        enable_images: query.enable_images,
        image_type_limit: query.image_type_limit,
        enable_image_types: query.enable_image_types,
        filters: query.filters,
        is_favorite: query.is_favorite,
        is_played: query.is_played,
        ..ItemsQuery::default()
    }
}

fn append_filter(existing: Option<String>, filter: &str) -> String {
    match existing.map(|value| value.trim().to_owned()) {
        Some(existing) if !existing.is_empty() => format!("{existing},{filter}"),
        _ => filter.to_owned(),
    }
}

async fn preferred_home_section_ids(
    repository: &UsersRepository,
    user_internal_id: i64,
) -> Vec<String> {
    // Emby Web：DisplayPreferences/usersettings?client=emby 的 CustomPrefs.homesectionN
    for client in ["emby", "Unknown", "FBZ"] {
        if let Ok(Some(stored)) = repository
            .find_display_preferences(user_internal_id, client, "usersettings")
            .await
        {
            let prefs = stored.custom_prefs.as_object();
            if let Some(object) = prefs {
                let mut ordered = Vec::new();
                for index in 0..32 {
                    let key = format!("homesection{index}");
                    let Some(value) = object.get(&key).and_then(|value| value.as_str()) else {
                        break;
                    };
                    let trimmed = value.trim();
                    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
                        continue;
                    }
                    ordered.push(normalize_home_section_pref(trimmed));
                }
                if !ordered.is_empty() {
                    return ordered;
                }
            }
        }
    }

    // 回退：UserConfiguration.OrderedViews 若含已知首页分区 id 则按其序。
    if let Ok(Some(value)) = repository
        .find_user_setting(user_internal_id, "emby.user.configuration")
        .await
    {
        if let Some(views) = value
            .get("OrderedViews")
            .or_else(|| value.get("orderedViews"))
            .and_then(|value| value.as_array())
        {
            let ordered: Vec<String> = views
                .iter()
                .filter_map(|value| value.as_str())
                .map(normalize_home_section_pref)
                .filter(|id| known_home_section_id(id))
                .collect();
            if !ordered.is_empty() {
                return ordered;
            }
        }
    }

    Vec::new()
}

fn normalize_home_section_pref(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace('_', "")
        .replace('-', "")
}

fn known_home_section_id(id: &str) -> bool {
    matches!(
        id,
        "latestmedia"
            | "latest"
            | "recentlyadded"
            | "resume"
            | "continuewatching"
            | "favorites"
            | "favourites"
            | "movies"
            | "series"
            | "tvshows"
            | "music"
            | "librarybuttons"
            | "smalllibrarytiles"
    )
}

fn order_home_sections(
    defaults: Vec<ContentSectionDto>,
    preferred_ids: Vec<String>,
) -> Vec<ContentSectionDto> {
    if preferred_ids.is_empty() {
        return defaults;
    }

    let mut by_id: std::collections::BTreeMap<String, ContentSectionDto> = defaults
        .into_iter()
        .map(|section| (normalize_home_section_pref(&section.id), section))
        .collect();

    let mut ordered = Vec::new();
    for pref in preferred_ids {
        let key = match pref.as_str() {
            "latest" | "recentlyadded" => "latestmedia".to_owned(),
            "continuewatching" => "resume".to_owned(),
            "favourites" => "favorites".to_owned(),
            "tvshows" => "series".to_owned(),
            "librarybuttons" | "smalllibrarytiles" => continue,
            other => other.to_owned(),
        };
        if let Some(section) = by_id.remove(&key) {
            ordered.push(section);
        }
    }
    for leftover in by_id.into_values() {
        ordered.push(leftover);
    }
    ordered
}

fn normalized_section_id(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("content section id is required"));
    }

    if value.len() > MAX_CONTENT_SECTION_ID_LEN
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Err(AppError::unprocessable("content section id is invalid"));
    }

    Ok(value.to_ascii_lowercase().replace('_', "-"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn order_home_sections_applies_preferred_ids_then_leftovers() {
        let ordered = order_home_sections(
            default_home_sections(),
            vec![
                "resume".to_owned(),
                "favorites".to_owned(),
                "librarybuttons".to_owned(),
            ],
        );
        assert_eq!(ordered[0].id, "resume");
        assert_eq!(ordered[1].id, "favorites");
        assert!(ordered.iter().any(|section| section.id == "latestmedia"));
        assert_eq!(ordered.len(), default_home_sections().len());
    }

    #[test]
    fn home_sections_serialize_official_content_section_shape() {
        let value = serde_json::to_value(default_home_sections()).unwrap();

        assert_eq!(value[0]["Id"], "latestmedia");
        assert_eq!(value[0]["Name"], "Latest media");
        assert_eq!(value[0]["SectionType"], "LatestMedia");
        assert_eq!(value[0]["ViewType"], "Items");
        assert_eq!(value[0]["ScrollDirection"], "Horizontal");
        assert_eq!(value[0]["CollectionType"], "mixed");
        assert_eq!(value[0]["CardSizeOffset"], 0);
        assert_eq!(value[0]["RefreshInterval"], 0);
        assert_eq!(value[0]["Monitor"], false);
    }

    #[test]
    fn content_section_items_map_known_sections_to_safe_item_queries() {
        let latest = section_items_query(
            "latestmedia",
            ContentSectionItemsQuery {
                limit: Some(500),
                fields: Some("PrimaryImageAspectRatio".to_owned()),
                ..ContentSectionItemsQuery::default()
            },
        )
        .expect("latest section should map");
        assert_eq!(latest.limit, Some(500));
        assert_eq!(latest.recursive, Some(true));
        assert_eq!(latest.sort_by.as_deref(), Some("DateCreated"));
        assert_eq!(latest.sort_order.as_deref(), Some("Descending"));
        assert_eq!(latest.fields.as_deref(), Some("PrimaryImageAspectRatio"));

        let resume = section_items_query("resume", ContentSectionItemsQuery::default())
            .expect("resume section should map");
        assert_eq!(resume.filters.as_deref(), Some("IsResumable"));

        let movies = section_items_query("movies", ContentSectionItemsQuery::default())
            .expect("movies section should map");
        assert_eq!(movies.include_item_types.as_deref(), Some("Movie"));

        let music = section_items_query("music", ContentSectionItemsQuery::default())
            .expect("music section should map");
        assert_eq!(music.include_item_types.as_deref(), Some("Audio"));
        assert_eq!(music.media_types.as_deref(), Some("Audio"));
    }

    #[test]
    fn content_section_items_query_accepts_lower_camel_and_snake_case_fields() {
        let lower_camel_uri: http::Uri = "/emby/Users/user-1/Sections/music/Items?parentId=library-1&startIndex=5&limit=20&recursive=true&includeItemTypes=Audio&mediaTypes=Audio&sortBy=SortName&sortOrder=Ascending&fields=MediaSources&enableImages=true&imageTypeLimit=1&enableImageTypes=Primary&isFavorite=true&isPlayed=false"
            .parse()
            .unwrap();
        let Query(lower_camel) =
            Query::<ContentSectionItemsQuery>::try_from_uri(&lower_camel_uri).unwrap();

        assert_eq!(lower_camel.parent_id.as_deref(), Some("library-1"));
        assert_eq!(lower_camel.start_index, Some(5));
        assert_eq!(lower_camel.limit, Some(20));
        assert_eq!(lower_camel.recursive, Some(true));
        assert_eq!(lower_camel.include_item_types.as_deref(), Some("Audio"));
        assert_eq!(lower_camel.media_types.as_deref(), Some("Audio"));
        assert_eq!(lower_camel.sort_by.as_deref(), Some("SortName"));
        assert_eq!(lower_camel.sort_order.as_deref(), Some("Ascending"));
        assert_eq!(lower_camel.fields.as_deref(), Some("MediaSources"));
        assert_eq!(lower_camel.enable_images, Some(true));
        assert_eq!(lower_camel.image_type_limit, Some(1));
        assert_eq!(lower_camel.enable_image_types.as_deref(), Some("Primary"));
        assert_eq!(lower_camel.is_favorite, Some(true));
        assert_eq!(lower_camel.is_played, Some(false));

        let snake_case_uri: http::Uri = "/Users/user-1/Sections/resume/Items?parent_id=library-1&start_index=2&include_item_types=Movie&media_types=Video&sort_by=DateCreated&sort_order=Descending&enable_images=false&image_type_limit=2&enable_image_types=Primary,Backdrop&is_favorite=false&is_played=true"
            .parse()
            .unwrap();
        let Query(snake_case) =
            Query::<ContentSectionItemsQuery>::try_from_uri(&snake_case_uri).unwrap();

        assert_eq!(snake_case.parent_id.as_deref(), Some("library-1"));
        assert_eq!(snake_case.start_index, Some(2));
        assert_eq!(snake_case.include_item_types.as_deref(), Some("Movie"));
        assert_eq!(snake_case.media_types.as_deref(), Some("Video"));
        assert_eq!(snake_case.sort_by.as_deref(), Some("DateCreated"));
        assert_eq!(snake_case.sort_order.as_deref(), Some("Descending"));
        assert_eq!(snake_case.enable_images, Some(false));
        assert_eq!(snake_case.image_type_limit, Some(2));
        assert_eq!(
            snake_case.enable_image_types.as_deref(),
            Some("Primary,Backdrop")
        );
        assert_eq!(snake_case.is_favorite, Some(false));
        assert_eq!(snake_case.is_played, Some(true));
    }

    #[test]
    fn unknown_section_returns_empty_boundary() {
        assert!(section_items_query("unknown", ContentSectionItemsQuery::default()).is_none());
    }

    #[test]
    fn content_section_id_is_bounded_and_path_safe() {
        assert_eq!(
            normalized_section_id(" Latest_Media ").unwrap(),
            "latest-media"
        );

        let err = normalized_section_id("").unwrap_err();
        assert_eq!(err.status_code(), http::StatusCode::UNPROCESSABLE_ENTITY);

        let err = normalized_section_id("bad/section").unwrap_err();
        assert_eq!(err.status_code(), http::StatusCode::UNPROCESSABLE_ENTITY);

        let err = normalized_section_id(&"x".repeat(MAX_CONTENT_SECTION_ID_LEN + 1)).unwrap_err();
        assert_eq!(err.status_code(), http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn content_sections_response_is_array_not_query_result() {
        let value = serde_json::to_value(default_home_sections()).unwrap();

        assert!(value.is_array());
        assert_eq!(value.get("Items"), None);
        assert_ne!(value, json!({ "Items": [] }));
    }
}
