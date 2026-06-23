use axum::{Json, response::IntoResponse};
use http::header::{CACHE_CONTROL, CONTENT_TYPE};

use crate::compat::emby::dto::BrandingOptionsDto;

pub async fn branding_configuration() -> Json<BrandingOptionsDto> {
    Json(BrandingOptionsDto::default())
}

pub async fn branding_css() -> impl IntoResponse {
    (
        [
            (CONTENT_TYPE, "text/css; charset=utf-8"),
            (CACHE_CONTROL, "no-store"),
        ],
        "",
    )
}
