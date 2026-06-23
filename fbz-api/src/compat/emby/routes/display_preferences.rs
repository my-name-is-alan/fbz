use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    compat::emby::dto::{DisplayPreferencesDto, DisplayPreferencesSource},
    error::AppError,
    state::AppState,
};

use super::access::authenticate_request_user;

const DEFAULT_DISPLAY_CLIENT: &str = "Unknown";

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DisplayPreferencesQuery {
    pub user_id: Option<String>,
    pub client: Option<String>,
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

fn normalized_client(client: Option<String>) -> String {
    client
        .and_then(|client| {
            let trimmed = client.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        })
        .unwrap_or_else(|| DEFAULT_DISPLAY_CLIENT.to_owned())
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
}
