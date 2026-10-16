use std::time::Duration;

use axum::{
    Json, Router,
    extract::State,
    http::{Request, Response, StatusCode},
    response::IntoResponse,
    routing::get,
};
use serde::Serialize;
use tower_http::{
    cors::CorsLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use tracing::{info, info_span, warn};

use crate::{
    admin,
    compat::emby,
    error::AppError,
    music, navigation, plugins, setup,
    state::{AppState, RuntimeReadinessSnapshot},
    users,
};

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    listen_addr: String,
    node_role: &'static str,
}

#[derive(Serialize)]
struct ReadyResponse {
    status: &'static str,
    service: &'static str,
    node_role: &'static str,
    readiness_timeout_ms: u64,
    checks: ReadyChecks,
    runtime: RuntimeReadinessSnapshot,
}

#[derive(Serialize)]
struct ReadyChecks {
    config: &'static str,
    database: &'static str,
    redis: &'static str,
}

pub fn build_router(state: AppState) -> Router {
    let slow_log_threshold_ms = state.config().telemetry.http_slow_log_threshold_ms;
    let router = Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .merge(admin::routes::router())
        .merge(crate::storage::router())
        .merge(crate::presentation::router())
        .merge(crate::media_bytes::router())
        .merge(navigation::routes::router())
        .merge(users::routes::router())
        .merge(setup::routes::router())
        .merge(music::routes::router())
        .merge(plugins::routes::router())
        .merge(plugins::host::router())
        .merge(emby::routes::router())
        .fallback(not_found)
        .with_state(state);

    http_trace_layer(router, slow_log_threshold_ms)
}

fn http_trace_layer(router: Router, slow_log_threshold_ms: u64) -> Router {
    router
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<_>| {
                    info_span!(
                        "http.request",
                        method = %request.method(),
                        uri = %request.uri().path(),
                        version = ?request.version(),
                    )
                })
                .on_response(
                    move |response: &Response<_>, latency: Duration, _span: &tracing::Span| {
                        let latency_ms = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX);
                        if latency_ms >= slow_log_threshold_ms {
                            warn!(
                                status = response.status().as_u16(),
                                latency_ms,
                                threshold_ms = slow_log_threshold_ms,
                                "slow http request"
                            );
                        } else {
                            info!(
                                status = response.status().as_u16(),
                                latency_ms, "http request completed"
                            );
                        }
                    },
                ),
        )
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(CorsLayer::permissive())
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "fbz-api",
        listen_addr: state.config().socket_addr().to_string(),
        node_role: state.config().node.role.as_str(),
    })
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let readiness = state.readiness().await;
    let is_ready = readiness.is_ready();
    let status_code = if is_ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(ReadyResponse {
            status: if is_ready { "ok" } else { "not_ready" },
            service: "fbz-api",
            node_role: state.config().node.role.as_str(),
            readiness_timeout_ms: state.config().server.readiness_timeout_ms,
            checks: ReadyChecks {
                config: "ok",
                database: readiness.database.as_str(),
                redis: readiness.redis.as_str(),
            },
            runtime: readiness.runtime,
        }),
    )
}

async fn not_found() -> AppError {
    AppError::not_found("route not found")
}

#[cfg(test)]
#[path = "app_router_tests.rs"]
mod tests;
