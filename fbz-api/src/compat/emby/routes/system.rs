use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
};

use crate::{
    compat::emby::dto::{
        EndpointInfoDto, PublicSystemInfoDto, ServerConfigurationDto, ServerConfigurationSource,
        ServerInfoSource, SystemInfoDto, WakeOnLanInfoDto,
    },
    error::AppError,
    state::AppState,
};

use super::access::authenticate_request_user;

pub async fn system_info(State(state): State<AppState>) -> Json<SystemInfoDto> {
    Json(SystemInfoDto::from(server_info_source(&state)))
}

pub async fn public_system_info(State(state): State<AppState>) -> Json<PublicSystemInfoDto> {
    Json(PublicSystemInfoDto::from(server_info_source(&state)))
}

pub async fn system_endpoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<EndpointInfoDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(EndpointInfoDto::conservative_default()))
}

pub async fn system_configuration(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ServerConfigurationDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(ServerConfigurationDto::from(
        server_configuration_source(&state),
    )))
}

pub async fn wake_on_lan_info(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<WakeOnLanInfoDto>>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(Vec::new()))
}

pub async fn system_ping() -> Response {
    (StatusCode::OK, "").into_response()
}

fn server_info_source(state: &AppState) -> ServerInfoSource {
    ServerInfoSource {
        id: "fbz-api".to_owned(),
        server_name: "FBZ".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        local_address: state.config().server.public_base_url.clone(),
        operating_system: std::env::consts::OS.to_owned(),
    }
}

fn server_configuration_source(state: &AppState) -> ServerConfigurationSource {
    ServerConfigurationSource {
        server_name: "FBZ".to_owned(),
        public_base_url: state.config().server.public_base_url.clone(),
        http_server_port_number: i32::from(state.config().server.port),
        cache_path: state
            .config()
            .storage
            .artwork_cache_dir
            .display()
            .to_string(),
        metadata_path: state
            .config()
            .storage
            .artwork_cache_dir
            .display()
            .to_string(),
        simultaneous_stream_limit: i32::from(state.config().transcode.max_concurrent),
    }
}
