use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::Deserialize;
use std::collections::BTreeMap;

use crate::{
    compat::emby::dto::{DisplayPreferencesDto, DisplayPreferencesSource},
    compat::emby::payload::parse_emby_body,
    error::AppError,
    state::AppState,
};

use super::access::authenticate_request_user;
use super::access::authenticate_route_user;

const DEFAULT_DISPLAY_CLIENT: &str = "Unknown";
const MAX_DISPLAY_PREF_TEXT_LEN: usize = 128;
const MAX_DISPLAY_PREF_VALUE_LEN: usize = 256;
const MAX_DISPLAY_PREFS: usize = 128;
const MAX_USER_SETTINGS_PARTIAL_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DisplayPreferencesQuery {
    pub user_id: Option<String>,
    pub client: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DisplayPreferencesUpdateDto {
    pub id: Option<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub custom_prefs: Option<BTreeMap<String, String>>,
    pub client: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct UserSettingUpdateDto {
    pub name: Option<String>,
    pub key: Option<String>,
    pub value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DisplayPreferencesUpdateInput {
    display_preferences_id: String,
    user_id: Option<String>,
    client: String,
    sort_by: Option<String>,
    sort_order: Option<String>,
    custom_prefs: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UserSettingsUpdateInput {
    user_id: String,
    settings: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TypedSettingPathInput {
    user_id: String,
    key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TrackSelectionInput {
    user_id: String,
    track_type: String,
}

pub async fn display_preferences(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<DisplayPreferencesQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<DisplayPreferencesDto>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    Ok(Json(DisplayPreferencesDto::from(
        DisplayPreferencesSource {
            id: item_id,
            client: normalized_client(query.client),
        },
    )))
}

pub async fn update_display_preferences(
    State(state): State<AppState>,
    Path(display_preferences_id): Path<String>,
    Query(query): Query<DisplayPreferencesQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id.as_deref()
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    let request: DisplayPreferencesUpdateDto = parse_emby_body(&headers, &body)?;
    let _input = display_preferences_update_input(&display_preferences_id, query, request)?;

    Ok(StatusCode::OK)
}

pub async fn user_settings(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<BTreeMap<String, String>>, AppError> {
    authenticate_route_user(&state, &user_id, &headers, &uri).await?;

    Ok(Json(BTreeMap::new()))
}

pub async fn update_user_settings(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    let request: Vec<UserSettingUpdateDto> = parse_emby_body(&headers, &body)?;
    let _input = user_settings_update_input(&user_id, request)?;

    Ok(StatusCode::OK)
}

pub async fn update_user_settings_partial(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    if body.len() > MAX_USER_SETTINGS_PARTIAL_BYTES {
        return Err(AppError::unprocessable(
            "user settings payload is too large",
        ));
    }
    let _user_id = normalized_required_text("UserId", Some(user_id), MAX_DISPLAY_PREF_TEXT_LEN)?;

    Ok(StatusCode::OK)
}

pub async fn typed_setting(
    State(state): State<AppState>,
    Path((user_id, key)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<BTreeMap<String, String>>, AppError> {
    authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    let _input = typed_setting_path_input(&user_id, &key)?;

    Ok(Json(BTreeMap::new()))
}

pub async fn update_typed_setting(
    State(state): State<AppState>,
    Path((user_id, key)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    let _input = typed_setting_path_input(&user_id, &key)?;
    ensure_typed_setting_body_size(&body)?;

    Ok(StatusCode::OK)
}

pub async fn clear_track_selection(
    State(state): State<AppState>,
    Path((user_id, track_type)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    let _input = track_selection_input(&user_id, &track_type)?;

    Ok(StatusCode::OK)
}

fn normalized_client(client: Option<String>) -> String {
    client
        .and_then(|client| {
            let trimmed = client.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        })
        .unwrap_or_else(|| DEFAULT_DISPLAY_CLIENT.to_owned())
}

fn display_preferences_update_input(
    display_preferences_id: &str,
    query: DisplayPreferencesQuery,
    request: DisplayPreferencesUpdateDto,
) -> Result<DisplayPreferencesUpdateInput, AppError> {
    let display_preferences_id = normalized_required_text(
        "DisplayPreferencesId",
        Some(display_preferences_id.to_owned()),
        MAX_DISPLAY_PREF_TEXT_LEN,
    )?;
    let client = normalized_client(request.client.or(query.client));
    let sort_order = normalized_sort_order(request.sort_order)?;

    Ok(DisplayPreferencesUpdateInput {
        display_preferences_id,
        user_id: normalized_optional_text(query.user_id, MAX_DISPLAY_PREF_TEXT_LEN),
        client,
        sort_by: normalized_optional_text(request.sort_by, MAX_DISPLAY_PREF_TEXT_LEN),
        sort_order,
        custom_prefs: normalized_custom_prefs(request.custom_prefs),
    })
}

fn user_settings_update_input(
    user_id: &str,
    request: Vec<UserSettingUpdateDto>,
) -> Result<UserSettingsUpdateInput, AppError> {
    let user_id = normalized_required_text(
        "UserId",
        Some(user_id.to_owned()),
        MAX_DISPLAY_PREF_TEXT_LEN,
    )?;
    let settings = request
        .into_iter()
        .filter_map(|setting| {
            let key =
                normalized_optional_text(setting.name.or(setting.key), MAX_DISPLAY_PREF_TEXT_LEN)?;
            let value = normalized_optional_text(setting.value, MAX_DISPLAY_PREF_VALUE_LEN)?;
            Some((key, value))
        })
        .take(MAX_DISPLAY_PREFS)
        .collect();

    Ok(UserSettingsUpdateInput { user_id, settings })
}

fn typed_setting_path_input(user_id: &str, key: &str) -> Result<TypedSettingPathInput, AppError> {
    let user_id = normalized_required_text(
        "UserId",
        Some(user_id.to_owned()),
        MAX_DISPLAY_PREF_TEXT_LEN,
    )?;
    let key = normalized_required_text("Key", Some(key.to_owned()), MAX_DISPLAY_PREF_TEXT_LEN)?;

    Ok(TypedSettingPathInput { user_id, key })
}

fn track_selection_input(user_id: &str, track_type: &str) -> Result<TrackSelectionInput, AppError> {
    let user_id = normalized_required_text(
        "UserId",
        Some(user_id.to_owned()),
        MAX_DISPLAY_PREF_TEXT_LEN,
    )?;
    let track_type = normalized_required_text(
        "TrackType",
        Some(track_type.to_owned()),
        MAX_DISPLAY_PREF_TEXT_LEN,
    )?;
    let track_type = if track_type.eq_ignore_ascii_case("Audio") {
        "Audio".to_owned()
    } else if track_type.eq_ignore_ascii_case("Subtitle") {
        "Subtitle".to_owned()
    } else {
        return Err(AppError::unprocessable("TrackType is invalid"));
    };

    Ok(TrackSelectionInput {
        user_id,
        track_type,
    })
}

fn ensure_typed_setting_body_size(body: &Bytes) -> Result<(), AppError> {
    if body.len() > MAX_USER_SETTINGS_PARTIAL_BYTES {
        return Err(AppError::unprocessable(
            "typed setting payload is too large",
        ));
    }

    Ok(())
}

fn normalized_sort_order(value: Option<String>) -> Result<Option<String>, AppError> {
    let Some(value) = normalized_optional_text(value, MAX_DISPLAY_PREF_TEXT_LEN) else {
        return Ok(None);
    };

    if value.eq_ignore_ascii_case("Ascending") {
        return Ok(Some("Ascending".to_owned()));
    }
    if value.eq_ignore_ascii_case("Descending") {
        return Ok(Some("Descending".to_owned()));
    }

    Err(AppError::unprocessable("SortOrder is invalid"))
}

fn normalized_custom_prefs(value: Option<BTreeMap<String, String>>) -> BTreeMap<String, String> {
    value
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(key, value)| {
            let key = normalized_optional_text(Some(key), MAX_DISPLAY_PREF_TEXT_LEN)?;
            let value = normalized_optional_text(Some(value), MAX_DISPLAY_PREF_VALUE_LEN)?;
            Some((key, value))
        })
        .take(MAX_DISPLAY_PREFS)
        .collect()
}

fn normalized_required_text(
    name: &str,
    value: Option<String>,
    max_len: usize,
) -> Result<String, AppError> {
    normalized_optional_text(value, max_len)
        .ok_or_else(|| AppError::unprocessable(format!("{name} is required")))
}

fn normalized_optional_text(value: Option<String>, max_len: usize) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.chars().take(max_len).collect())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_client_defaults_when_missing_or_blank() {
        assert_eq!(normalized_client(None), DEFAULT_DISPLAY_CLIENT);
        assert_eq!(
            normalized_client(Some("  ".to_owned())),
            DEFAULT_DISPLAY_CLIENT
        );
        assert_eq!(normalized_client(Some(" Infuse ".to_owned())), "Infuse");
    }

    #[test]
    fn display_preferences_update_body_normalizes_supported_fields() {
        let request = serde_json::from_value::<DisplayPreferencesUpdateDto>(serde_json::json!({
            "Id": " body-id ",
            "SortBy": " DateCreated ",
            "SortOrder": "descending",
            "Client": " Infuse ",
            "CustomPrefs": {
                "view": "poster",
                "blank": "   ",
                "long": "x".repeat(300)
            }
        }))
        .expect("display preferences body should deserialize");

        let input = display_preferences_update_input(
            " path-id ",
            DisplayPreferencesQuery {
                user_id: Some("user-1".to_owned()),
                client: Some(" QueryClient ".to_owned()),
            },
            request,
        )
        .expect("display preferences update should normalize");

        assert_eq!(input.display_preferences_id, "path-id");
        assert_eq!(input.user_id.as_deref(), Some("user-1"));
        assert_eq!(input.client, "Infuse");
        assert_eq!(input.sort_by.as_deref(), Some("DateCreated"));
        assert_eq!(input.sort_order.as_deref(), Some("Descending"));
        assert_eq!(
            input.custom_prefs.get("view").map(String::as_str),
            Some("poster")
        );
        assert!(!input.custom_prefs.contains_key("blank"));
        assert_eq!(input.custom_prefs["long"].len(), 256);
    }

    #[test]
    fn user_settings_update_body_normalizes_key_value_pairs() {
        let request = vec![
            UserSettingUpdateDto {
                name: Some(" theme ".to_owned()),
                key: None,
                value: Some(" dark ".to_owned()),
            },
            UserSettingUpdateDto {
                name: None,
                key: Some("empty".to_owned()),
                value: Some("   ".to_owned()),
            },
            UserSettingUpdateDto {
                name: None,
                key: Some("long".to_owned()),
                value: Some("x".repeat(300)),
            },
        ];

        let input = user_settings_update_input(" user-1 ", request)
            .expect("user settings update should normalize");

        assert_eq!(input.user_id, "user-1");
        assert_eq!(
            input.settings.get("theme").map(String::as_str),
            Some("dark")
        );
        assert!(!input.settings.contains_key("empty"));
        assert_eq!(input.settings["long"].len(), 256);
    }

    #[test]
    fn typed_setting_path_normalizes_user_and_key() {
        let input = typed_setting_path_input(" user-1 ", " playback.audio ")
            .expect("typed setting path should normalize");

        assert_eq!(input.user_id, "user-1");
        assert_eq!(input.key, "playback.audio");
        assert!(typed_setting_path_input(" ", "playback.audio").is_err());
        assert!(typed_setting_path_input("user-1", " ").is_err());
    }

    #[test]
    fn track_selection_input_accepts_audio_and_subtitle_only() {
        let audio = track_selection_input(" user-1 ", " audio ")
            .expect("audio track selection should normalize");
        let subtitle = track_selection_input(" user-1 ", "Subtitle")
            .expect("subtitle track selection should normalize");

        assert_eq!(audio.user_id, "user-1");
        assert_eq!(audio.track_type, "Audio");
        assert_eq!(subtitle.track_type, "Subtitle");
        assert!(track_selection_input("user-1", "video").is_err());
    }

    #[test]
    fn typed_setting_body_rejects_oversized_payload() {
        ensure_typed_setting_body_size(&Bytes::from(vec![0; MAX_USER_SETTINGS_PARTIAL_BYTES]))
            .expect("max sized typed setting payload should pass");
        assert!(
            ensure_typed_setting_body_size(&Bytes::from(vec![
                0;
                MAX_USER_SETTINGS_PARTIAL_BYTES + 1
            ]))
            .is_err()
        );
    }
}
