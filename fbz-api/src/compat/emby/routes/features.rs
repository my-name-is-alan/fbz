use axum::{
    Json,
    extract::State,
    http::{HeaderMap, Uri},
};
use serde::Serialize;

use crate::{error::AppError, state::AppState};

use super::access::authenticate_request_user;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct FeatureInfoDto {
    pub name: String,
    pub id: String,
    pub feature_type: FeatureTypeDto,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FeatureTypeDto {
    System,
    User,
}

pub async fn features(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<FeatureInfoDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if !user.can_manage_server() {
        return Err(AppError::forbidden("server management permission required"));
    }

    Ok(Json(feature_items()))
}

fn feature_items() -> Vec<FeatureInfoDto> {
    vec![
        FeatureInfoDto {
            name: "FBZ Core".to_owned(),
            id: "fbz-core".to_owned(),
            feature_type: FeatureTypeDto::System,
        },
        FeatureInfoDto {
            name: "Emby Compatibility".to_owned(),
            id: "emby-compatibility".to_owned(),
            feature_type: FeatureTypeDto::System,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_items_expose_stable_system_features() {
        let features = feature_items();

        assert_eq!(features.len(), 2);
        assert_eq!(features[0].id, "fbz-core");
        assert_eq!(features[0].feature_type, FeatureTypeDto::System);
        assert_eq!(features[1].id, "emby-compatibility");
        assert!(
            features
                .iter()
                .all(|feature| feature.feature_type == FeatureTypeDto::System)
        );
    }

    #[test]
    fn feature_info_serializes_pascal_case_with_official_enum_values() {
        let value = serde_json::to_value(&feature_items()[0]).unwrap();

        assert_eq!(value["Name"], "FBZ Core");
        assert_eq!(value["Id"], "fbz-core");
        assert_eq!(value["FeatureType"], "System");
    }
}
