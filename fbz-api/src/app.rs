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
                        uri = %request.uri(),
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
mod tests {
    use axum::{
        body::{self, Body},
        http::{Method, Request, StatusCode, header::CONTENT_TYPE},
    };
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;

    #[test]
    fn http_trace_layer_uses_configured_slow_request_logging() {
        let source = include_str!("app.rs");
        let production_source = source
            .split("#[cfg(test)]")
            .next()
            .expect("app source should include production section");

        assert!(production_source.contains("fn http_trace_layer"));
        assert!(production_source.contains("http_slow_log_threshold_ms"));
        assert!(production_source.contains("TraceLayer::new_for_http()"));
        assert!(production_source.contains("make_span_with"));
        assert!(production_source.contains("on_response"));
        assert!(production_source.contains("latency_ms"));
        assert!(production_source.contains("threshold_ms"));
        assert!(production_source.contains("slow http request"));
        assert!(!production_source.contains(".layer(TraceLayer::new_for_http())"));
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let app = build_router(AppState::for_tests(Config::default()));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("health request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ready_returns_service_unavailable_without_dependencies() {
        let mut config = Config::default();
        config.server.readiness_timeout_ms = 750;
        let app = build_router(AppState::for_tests(config));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("ready request should succeed");

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should be readable");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("body should be json");

        assert_eq!(json["status"], "not_ready");
        assert_eq!(json["service"], "fbz-api");
        assert_eq!(json["node_role"], "all");
        assert_eq!(json["readiness_timeout_ms"], 750);
        assert_eq!(json["checks"]["config"], "ok");
        assert_eq!(json["checks"]["database"], "not_configured");
        assert_eq!(json["checks"]["redis"], "not_configured");
        assert_eq!(json["runtime"]["roles"]["api"], true);
        assert_eq!(json["runtime"]["roles"]["worker"], true);
        assert_eq!(json["runtime"]["roles"]["scheduler"], true);
        assert_eq!(json["runtime"]["queues"]["status"], "not_configured");
        assert_eq!(
            json["runtime"]["queues"]["event_stream_mirror"]["unmirrored"],
            0
        );
        assert_eq!(
            json["runtime"]["queues"]["event_stream_mirror"]["claimable"],
            0
        );

        let workers = json["runtime"]["workers"]
            .as_array()
            .expect("workers should be an array");
        assert!(workers.iter().any(|worker| {
            worker["name"] == "scan"
                && worker["enabled"] == false
                && worker["should_run"] == false
                && worker["interval_seconds"] == 5
        }));
        assert!(workers.iter().any(|worker| {
            worker["name"] == "scheduler"
                && worker["enabled"] == false
                && worker["should_run"] == false
                && worker["interval_seconds"] == 5
        }));
    }

    #[tokio::test]
    async fn unknown_route_returns_structured_not_found() {
        let app = build_router(AppState::for_tests(Config::default()));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/missing")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("missing route request should succeed");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn emby_system_info_aliases_return_ok() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/System/Info",
            "/System/Info",
            "/emby/System/Info/Public",
            "/System/Info/Public",
            "/emby/System/Ping",
            "/System/Ping",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("system info request should succeed");

            assert_eq!(response.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn emby_system_release_notes_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/System/ReleaseNotes",
            "/System/ReleaseNotes",
            "/emby/System/ReleaseNotes/Versions",
            "/System/ReleaseNotes/Versions",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("system release notes request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_system_ping_head_aliases_return_ok() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in ["/emby/System/Ping", "/System/Ping"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::HEAD)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("system ping head request should succeed");

            assert_eq!(response.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn emby_system_endpoint_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/System/Endpoint",
            "/System/Endpoint",
            "/emby/System/Configuration",
            "/System/Configuration",
            "/emby/System/WakeOnLanInfo",
            "/System/WakeOnLanInfo",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("system endpoint request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_system_configuration_write_and_key_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri, body) in [
            (
                Method::POST,
                "/emby/System/Configuration",
                Body::from(r#"{"ServerName":"FBZ"}"#),
            ),
            (
                Method::POST,
                "/System/Configuration",
                Body::from(r#"{"ServerName":"FBZ"}"#),
            ),
            (
                Method::GET,
                "/emby/System/Configuration/system",
                Body::empty(),
            ),
            (Method::GET, "/System/Configuration/system", Body::empty()),
            (
                Method::POST,
                "/emby/System/Configuration/metadata",
                Body::from(r#"{"PreferredMetadataLanguage":"zh-CN"}"#),
            ),
            (
                Method::POST,
                "/System/Configuration/metadata",
                Body::from(r#"{"PreferredMetadataLanguage":"zh-CN"}"#),
            ),
            (
                Method::POST,
                "/emby/System/Configuration/Partial",
                Body::from(r#"{"ServerName":"FBZ"}"#),
            ),
            (
                Method::POST,
                "/System/Configuration/Partial",
                Body::from(r#"{"ServerName":"FBZ"}"#),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .header(CONTENT_TYPE, "application/json")
                        .body(body)
                        .expect("request should build"),
                )
                .await
                .expect("system configuration request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        }
    }

    #[tokio::test]
    async fn emby_system_power_control_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/System/Restart",
            "/System/Restart",
            "/emby/System/Shutdown",
            "/System/Shutdown",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("system power control request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        }
    }

    #[tokio::test]
    async fn emby_localization_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Localization/Countries",
            "/Localization/Countries",
            "/emby/Localization/Cultures",
            "/Localization/Cultures",
            "/emby/Localization/Options",
            "/Localization/Options",
            "/emby/Localization/ParentalRatings",
            "/Localization/ParentalRatings",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("localization request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_feature_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in ["/emby/Features", "/Features"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("features request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_items_latest_query_user_alias_exists() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/Latest?UserId=user-1&Limit=20",
            "/emby/Items/Latest?userId=user-1&parentId=library-1&startIndex=0&limit=20&includeItemTypes=Movie&sortBy=DateCreated&sortOrder=Descending&fields=MediaSources",
            "/Items/Latest?UserId=user-1&Limit=20",
            "/emby/Items/Resume?UserId=user-1&Limit=20",
            "/emby/Items/Resume?userId=user-1&parentId=library-1&startIndex=0&limit=20&includeItemTypes=Movie&sortBy=DateCreated&sortOrder=Descending&fields=MediaSources",
            "/Items/Resume?UserId=user-1&Limit=20",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("latest items request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_plugin_service_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri) in [
            (Method::GET, "/emby/Plugins"),
            (Method::GET, "/Plugins"),
            (Method::GET, "/emby/Plugins/plugin-1/Configuration"),
            (Method::GET, "/Plugins/plugin-1/Configuration"),
            (Method::POST, "/emby/Plugins/plugin-1/Configuration"),
            (Method::POST, "/Plugins/plugin-1/Configuration"),
            (Method::GET, "/emby/Plugins/plugin-1/Thumb"),
            (Method::GET, "/Plugins/plugin-1/Thumb"),
            (Method::DELETE, "/emby/Plugins/plugin-1"),
            (Method::DELETE, "/Plugins/plugin-1"),
            (Method::POST, "/emby/Plugins/plugin-1/Delete"),
            (Method::POST, "/Plugins/plugin-1/Delete"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("plugin service request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_package_service_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri) in [
            (
                Method::GET,
                "/emby/Packages?PackageType=System&TargetSystems=Server&IsPremium=false&IsAdult=false",
            ),
            (
                Method::GET,
                "/emby/Packages?packageType=System&targetSystems=Server&isPremium=false&isAdult=false",
            ),
            (Method::GET, "/Packages"),
            (Method::GET, "/emby/Packages/fbz-core"),
            (Method::GET, "/emby/Packages/fbz-core?assemblyGuid=fbz-core"),
            (Method::GET, "/Packages/fbz-core"),
            (
                Method::GET,
                "/emby/Packages/Updates?PackageType=UserInstalled",
            ),
            (Method::GET, "/emby/Packages/Updates?packageType=System"),
            (Method::GET, "/Packages/Updates?PackageType=System"),
            (
                Method::POST,
                "/emby/Packages/Installed/fbz-core?Version=1.0.0&UpdateClass=Release",
            ),
            (
                Method::POST,
                "/emby/Packages/Installed/fbz-core?assemblyGuid=fbz-core&version=1.0.0&updateClass=Release",
            ),
            (
                Method::POST,
                "/Packages/Installed/fbz-core?Version=1.0.0&UpdateClass=Release",
            ),
            (Method::DELETE, "/emby/Packages/Installing/install-1"),
            (Method::DELETE, "/Packages/Installing/install-1"),
            (Method::POST, "/emby/Packages/Installing/install-1/Delete"),
            (Method::POST, "/Packages/Installing/install-1/Delete"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("package service request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_dlna_profile_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri, body) in [
            (Method::GET, "/emby/Dlna/ProfileInfos", Body::empty()),
            (Method::GET, "/Dlna/ProfileInfos", Body::empty()),
            (Method::GET, "/emby/Dlna/Profiles/Default", Body::empty()),
            (Method::GET, "/Dlna/Profiles/Default", Body::empty()),
            (
                Method::GET,
                "/emby/Dlna/Profiles/fbz-default",
                Body::empty(),
            ),
            (Method::GET, "/Dlna/Profiles/fbz-default", Body::empty()),
            (
                Method::POST,
                "/emby/Dlna/Profiles",
                Body::from(r#"{"Name":"Custom Profile","Id":"custom-profile"}"#),
            ),
            (
                Method::POST,
                "/emby/Dlna/Profiles",
                Body::from(
                    r#"{"Name":"Custom Profile","Id":"custom-profile","friendlyName":"Living Room","manufacturerUrl":"https://example.test","modelName":"FBZ Client","supportedMediaTypes":"Audio,Video","maxStreamingBitrate":1000000,"protocolInfoDetection":{"enabledForVideo":true,"enabledForAudio":true,"enabledForPhotos":false},"directPlayProfiles":[{"container":"mp4","audioCodec":"aac","videoCodec":"h264","Type":"Video"}]}"#,
                ),
            ),
            (
                Method::POST,
                "/Dlna/Profiles",
                Body::from(r#"{"Name":"Custom Profile","Id":"custom-profile"}"#),
            ),
            (
                Method::POST,
                "/emby/Dlna/Profiles/custom-profile",
                Body::from(r#"{"Name":"Custom Profile","Id":"custom-profile"}"#),
            ),
            (
                Method::POST,
                "/Dlna/Profiles/custom-profile",
                Body::from(
                    r#"{"Name":"Custom Profile","Id":"custom-profile","friendly_name":"Living Room","manufacturer_url":"https://example.test","model_name":"FBZ Client","supported_media_types":"Audio,Video","max_streaming_bitrate":1000000,"protocol_info_detection":{"enabled_for_video":true,"enabled_for_audio":true,"enabled_for_photos":false},"direct_play_profiles":[{"container":"mp4","audio_codec":"aac","video_codec":"h264","Type":"Video"}]}"#,
                ),
            ),
            (
                Method::POST,
                "/Dlna/Profiles/custom-profile",
                Body::from(r#"{"Name":"Custom Profile","Id":"custom-profile"}"#),
            ),
            (
                Method::DELETE,
                "/emby/Dlna/Profiles/custom-profile",
                Body::empty(),
            ),
            (
                Method::DELETE,
                "/Dlna/Profiles/custom-profile",
                Body::empty(),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .header(CONTENT_TYPE, "application/json")
                        .body(body)
                        .expect("request should build"),
                )
                .await
                .expect("dlna profile request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_environment_service_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri, body) in [
            (
                Method::GET,
                "/emby/Environment/DefaultDirectoryBrowser",
                Body::empty(),
            ),
            (
                Method::GET,
                "/Environment/DefaultDirectoryBrowser",
                Body::empty(),
            ),
            (Method::GET, "/emby/Environment/Drives", Body::empty()),
            (Method::GET, "/Environment/Drives", Body::empty()),
            (
                Method::GET,
                "/emby/Environment/DirectoryContents?Path=.&IncludeFiles=false&IncludeDirectories=true",
                Body::empty(),
            ),
            (
                Method::GET,
                "/emby/Environment/DirectoryContents?path=.&includeFiles=false&includeDirectories=true",
                Body::empty(),
            ),
            (
                Method::GET,
                "/Environment/DirectoryContents?Path=.&IncludeFiles=false&IncludeDirectories=true",
                Body::empty(),
            ),
            (
                Method::POST,
                "/emby/Environment/DirectoryContents?Path=.&IncludeFiles=true&IncludeDirectories=true",
                Body::from(r#"{"Username":"","Password":""}"#),
            ),
            (
                Method::POST,
                "/emby/Environment/DirectoryContents?path=.&includeFiles=true&includeDirectories=true",
                Body::from(r#"{"username":"","password":""}"#),
            ),
            (
                Method::POST,
                "/Environment/DirectoryContents?Path=.&IncludeFiles=true&IncludeDirectories=true",
                Body::from(r#"{"Username":"","Password":""}"#),
            ),
            (
                Method::GET,
                "/emby/Environment/ParentPath?Path=.",
                Body::empty(),
            ),
            (
                Method::GET,
                "/emby/Environment/ParentPath?path=.",
                Body::empty(),
            ),
            (Method::GET, "/Environment/ParentPath?Path=.", Body::empty()),
            (
                Method::GET,
                "/emby/Environment/NetworkDevices",
                Body::empty(),
            ),
            (Method::GET, "/Environment/NetworkDevices", Body::empty()),
            (
                Method::GET,
                "/emby/Environment/NetworkShares?Path=server",
                Body::empty(),
            ),
            (
                Method::GET,
                "/emby/Environment/NetworkShares?path=server",
                Body::empty(),
            ),
            (
                Method::GET,
                "/Environment/NetworkShares?Path=server",
                Body::empty(),
            ),
            (
                Method::POST,
                "/emby/Environment/ValidatePath?Path=.",
                Body::from(r#"{"ValidateWriteable":false,"IsFile":false}"#),
            ),
            (
                Method::POST,
                "/emby/Environment/ValidatePath?path=.",
                Body::from(r#"{"validateWriteable":false,"isFile":false}"#),
            ),
            (
                Method::POST,
                "/Environment/ValidatePath?Path=.",
                Body::from(r#"{"ValidateWriteable":false,"IsFile":false}"#),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .header(CONTENT_TYPE, "application/json")
                        .body(body)
                        .expect("request should build"),
                )
                .await
                .expect("environment service request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_activity_log_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/System/ActivityLog/Entries?StartIndex=0&Limit=20&MinDate=2024-01-01T00:00:00Z",
            "/System/ActivityLog/Entries?StartIndex=0&Limit=20&MinDate=2024-01-01T00:00:00Z",
            "/emby/System/ActivityLog/Entries?startIndex=0&limit=20&minDate=2024-01-01T00:00:00Z",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("activity log request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_notification_service_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri, body) in [
            (Method::GET, "/emby/Notifications/Types", Body::empty()),
            (Method::GET, "/Notifications/Types", Body::empty()),
            (
                Method::POST,
                "/emby/Notifications/Admin?Name=Library&Description=Scan%20finished&Level=Info",
                Body::from(r#"{"DisplayDateTime":true}"#),
            ),
            (
                Method::POST,
                "/emby/Notifications/Admin?name=Library&description=Scan%20finished&imageUrl=https%3A%2F%2Fexample.test%2Fimage.png&url=https%3A%2F%2Fexample.test%2Factivity&level=Info",
                Body::from(r#"{"displayDateTime":true}"#),
            ),
            (
                Method::POST,
                "/Notifications/Admin?Name=Library&Description=Scan%20finished&Level=Info",
                Body::from(r#"{"DisplayDateTime":true}"#),
            ),
            (
                Method::GET,
                "/emby/Notifications/Services/Defaults",
                Body::empty(),
            ),
            (
                Method::GET,
                "/Notifications/Services/Defaults",
                Body::empty(),
            ),
            (
                Method::POST,
                "/emby/Notifications/Services/Test",
                Body::from(r#"{"NotifierKey":"fbz-host","Enabled":false}"#),
            ),
            (
                Method::POST,
                "/emby/Notifications/Services/Test",
                Body::from(
                    r#"{"notifierKey":"fbz-host","serviceName":"FBZ Host Notifications","pluginId":"fbz-core","friendlyName":"Host","id":"fbz-host","enabled":false,"userIds":["user-1"],"deviceIds":["device-1"],"libraryIds":["library-1"],"eventIds":["event-1"],"userId":"user-1","isSelfNotification":true,"groupItems":true,"options":{}}"#,
                ),
            ),
            (
                Method::POST,
                "/Notifications/Services/Test",
                Body::from(
                    r#"{"notifier_key":"fbz-host","service_name":"FBZ Host Notifications","plugin_id":"fbz-core","friendly_name":"Host","id":"fbz-host","enabled":false,"user_ids":["user-1"],"device_ids":["device-1"],"library_ids":["library-1"],"event_ids":["event-1"],"user_id":"user-1","is_self_notification":true,"group_items":true,"options":{}}"#,
                ),
            ),
            (
                Method::POST,
                "/Notifications/Services/Test",
                Body::from(r#"{"NotifierKey":"fbz-host","Enabled":false}"#),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .header(CONTENT_TYPE, "application/json")
                        .body(body)
                        .expect("request should build"),
                )
                .await
                .expect("notification service request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_encoding_option_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri, body) in [
            (
                Method::GET,
                "/emby/Encoding/CodecConfiguration/Defaults",
                Body::empty(),
            ),
            (
                Method::GET,
                "/Encoding/CodecConfiguration/Defaults",
                Body::empty(),
            ),
            (
                Method::GET,
                "/emby/Encoding/CodecInformation/Video",
                Body::empty(),
            ),
            (
                Method::GET,
                "/Encoding/CodecInformation/Video",
                Body::empty(),
            ),
            (Method::GET, "/emby/Encoding/ToneMapOptions", Body::empty()),
            (Method::GET, "/Encoding/ToneMapOptions", Body::empty()),
            (
                Method::GET,
                "/emby/Encoding/FullToneMapOptions",
                Body::empty(),
            ),
            (Method::GET, "/Encoding/FullToneMapOptions", Body::empty()),
            (
                Method::POST,
                "/emby/Encoding/FullToneMapOptions",
                Body::from(r#"{}"#),
            ),
            (
                Method::POST,
                "/Encoding/FullToneMapOptions",
                Body::from(r#"{}"#),
            ),
            (
                Method::GET,
                "/emby/Encoding/PublicToneMapOptions",
                Body::empty(),
            ),
            (Method::GET, "/Encoding/PublicToneMapOptions", Body::empty()),
            (
                Method::POST,
                "/emby/Encoding/PublicToneMapOptions",
                Body::from(r#"{}"#),
            ),
            (
                Method::POST,
                "/Encoding/PublicToneMapOptions",
                Body::from(r#"{}"#),
            ),
            (
                Method::GET,
                "/emby/Encoding/CodecParameters?CodecId=h264&ParameterContext=Encoding",
                Body::empty(),
            ),
            (
                Method::GET,
                "/emby/Encoding/CodecParameters?codecId=h264&parameterContext=Encoding",
                Body::empty(),
            ),
            (
                Method::GET,
                "/Encoding/CodecParameters?CodecId=h264&ParameterContext=Encoding",
                Body::empty(),
            ),
            (
                Method::POST,
                "/emby/Encoding/CodecParameters?CodecId=h264&ParameterContext=Encoding",
                Body::from(r#"{}"#),
            ),
            (
                Method::POST,
                "/emby/Encoding/CodecParameters?codecId=h264&parameterContext=Encoding",
                Body::from(r#"{}"#),
            ),
            (
                Method::POST,
                "/Encoding/CodecParameters?CodecId=h264&ParameterContext=Encoding",
                Body::from(r#"{}"#),
            ),
            (Method::GET, "/emby/Encoding/SubtitleOptions", Body::empty()),
            (Method::GET, "/Encoding/SubtitleOptions", Body::empty()),
            (
                Method::POST,
                "/emby/Encoding/SubtitleOptions",
                Body::from(r#"{}"#),
            ),
            (
                Method::POST,
                "/Encoding/SubtitleOptions",
                Body::from(r#"{}"#),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .header(CONTENT_TYPE, "application/json")
                        .body(body)
                        .expect("request should build"),
                )
                .await
                .expect("encoding options request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_branding_aliases_return_startup_defaults() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Branding/Configuration",
            "/Branding/Configuration",
            "/emby/Branding/Css",
            "/Branding/Css",
            "/emby/Branding/Css.css",
            "/Branding/Css.css",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("branding request should succeed");

            assert_eq!(response.status(), StatusCode::OK);

            if uri.contains("/Css") {
                let content_type = response
                    .headers()
                    .get(CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default();

                assert!(content_type.starts_with("text/css"));
            }
        }
    }

    #[tokio::test]
    async fn emby_live_tv_mutation_probes_return_controlled_conflict() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri) in [
            (Method::DELETE, "/emby/LiveTv/ChannelMappingOptions"),
            (Method::POST, "/LiveTv/ChannelMappingOptions"),
            (Method::PUT, "/emby/LiveTv/ChannelMappings"),
            (Method::DELETE, "/LiveTv/ChannelMappings"),
            (Method::POST, "/emby/LiveTv/ListingProviders"),
            (Method::DELETE, "/LiveTv/ListingProviders"),
            (Method::POST, "/emby/LiveTv/ListingProviders/Delete"),
            (
                Method::POST,
                "/LiveTv/Manage/Channels/channel-1/Disabled?Disabled=true",
            ),
            (
                Method::POST,
                "/emby/LiveTv/Manage/Channels/channel-1/SortIndex?SortIndex=10",
            ),
            (Method::DELETE, "/LiveTv/Recordings/recording-1"),
            (Method::POST, "/emby/LiveTv/Recordings/recording-1/Delete"),
            (Method::POST, "/LiveTv/SeriesTimers"),
            (Method::POST, "/emby/LiveTv/SeriesTimers/series-1"),
            (Method::DELETE, "/LiveTv/SeriesTimers/series-1"),
            (Method::POST, "/emby/LiveTv/SeriesTimers/series-1/Delete"),
            (Method::POST, "/LiveTv/Timers"),
            (Method::POST, "/emby/LiveTv/Timers/timer-1"),
            (Method::DELETE, "/LiveTv/Timers/timer-1"),
            (Method::POST, "/emby/LiveTv/Timers/timer-1/Delete"),
            (Method::POST, "/LiveTv/TunerHosts"),
            (Method::DELETE, "/emby/LiveTv/TunerHosts"),
            (Method::POST, "/LiveTv/TunerHosts/Delete"),
            (Method::POST, "/emby/LiveTv/Tuners/tuner-1/Reset"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .body(Body::from("{}"))
                        .expect("request should build"),
                )
                .await
                .expect("live tv mutation request should succeed");

            assert_eq!(response.status(), StatusCode::CONFLICT);
            let body = body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body should read");
            let value: serde_json::Value =
                serde_json::from_slice(&body).expect("body should be json");
            assert_eq!(value["error"]["code"], "conflict");
        }
    }

    #[tokio::test]
    async fn emby_user_library_access_write_probes_are_routed() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/Access",
            "/Items/Access",
            "/emby/Items/item-1/MakePrivate",
            "/Items/item-1/MakePrivate",
            "/emby/Items/item-1/MakePublic",
            "/Items/item-1/MakePublic",
            "/emby/Items/Shared/Leave",
            "/Items/Shared/Leave",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from("{}"))
                        .expect("request should build"),
                )
                .await
                .expect("user library write probe should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND, "uri {uri}");
            assert_ne!(
                response.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "uri {uri}"
            );
        }
    }

    #[tokio::test]
    async fn emby_live_tv_startup_and_setup_read_probes_are_controlled() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in ["/emby/LiveTv/GuideInfo", "/LiveTv/GuideInfo"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("live tv guide info request should succeed");

            assert_eq!(response.status(), StatusCode::OK);
            let body = body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body should read");
            let value: serde_json::Value =
                serde_json::from_slice(&body).expect("body should be json");
            assert_eq!(value["IsEnabled"], false);
            assert!(value["EnabledUsers"].as_array().is_some_and(Vec::is_empty));
        }

        for uri in [
            "/emby/LiveTv/EPG?UserId=user-1&StartIndex=0&Limit=10",
            "/LiveTv/EPG?UserId=user-1&StartIndex=0&Limit=10",
            "/emby/LiveTv/AvailableRecordingOptions?UserId=user-1",
            "/LiveTv/AvailableRecordingOptions?UserId=user-1",
            "/emby/LiveTv/ChannelTags?UserId=user-1",
            "/LiveTv/ChannelTags?UserId=user-1",
            "/emby/LiveTv/ChannelTags/Prefixes?UserId=user-1",
            "/LiveTv/ChannelTags/Prefixes?UserId=user-1",
            "/emby/LiveTv/ChannelMappingOptions",
            "/LiveTv/ChannelMappingOptions",
            "/emby/LiveTv/ChannelMappings",
            "/LiveTv/ChannelMappings",
            "/emby/LiveTv/ListingProviders",
            "/LiveTv/ListingProviders",
            "/emby/LiveTv/ListingProviders/Available",
            "/LiveTv/ListingProviders/Available",
            "/emby/LiveTv/ListingProviders/Lineups",
            "/LiveTv/ListingProviders/Lineups",
            "/emby/LiveTv/Manage/Channels",
            "/LiveTv/Manage/Channels",
            "/emby/LiveTv/TunerHosts",
            "/LiveTv/TunerHosts",
            "/emby/LiveTv/TunerHosts/Default/hdhomerun",
            "/LiveTv/TunerHosts/Default/hdhomerun",
            "/emby/LiveTv/TunerHosts/Types",
            "/LiveTv/TunerHosts/Types",
            "/emby/LiveTv/Tuners/Discover",
            "/LiveTv/Tuners/Discover",
            "/emby/LiveTv/Tuners/Discvover",
            "/LiveTv/Tuners/Discvover",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("live tv setup probe request should succeed");

            assert_eq!(response.status(), StatusCode::OK);
            let body = body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body should read");
            let value: serde_json::Value =
                serde_json::from_slice(&body).expect("body should be json");
            assert_eq!(value["TotalRecordCount"], 0);
            assert!(value["Items"].as_array().is_some_and(Vec::is_empty));
        }
    }

    #[tokio::test]
    async fn emby_live_tv_aliases_return_disabled_or_empty() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in ["/emby/LiveTv/Info", "/LiveTv/Info"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("live tv info request should succeed");

            assert_eq!(response.status(), StatusCode::OK);
            let body = body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body should read");
            let value: serde_json::Value =
                serde_json::from_slice(&body).expect("body should be json");
            assert_eq!(value["IsEnabled"], false);
            assert!(value["EnabledUsers"].as_array().is_some_and(Vec::is_empty));
        }

        for uri in ["/emby/LiveTv/Folder", "/LiveTv/Folder"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("live tv folder request should succeed");

            assert_eq!(response.status(), StatusCode::OK);
            let body = body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body should read");
            let value: serde_json::Value =
                serde_json::from_slice(&body).expect("body should be json");
            assert_eq!(value["Id"], "livetv");
            assert_eq!(value["Type"], "CollectionFolder");
            assert_eq!(value["CollectionType"], "livetv");
            assert_eq!(value["IsFolder"], true);
        }

        for uri in [
            "/emby/LiveTv/Channels?UserId=user-1&StartIndex=20&Limit=10",
            "/LiveTv/Channels?UserId=user-1&StartIndex=20&Limit=10",
            "/emby/LiveTv/Programs?UserId=user-1",
            "/LiveTv/Programs?UserId=user-1",
            "/emby/LiveTv/RecommendedPrograms?UserId=user-1",
            "/LiveTv/RecommendedPrograms?UserId=user-1",
            "/emby/LiveTv/UpcomingPrograms?UserId=user-1",
            "/LiveTv/UpcomingPrograms?UserId=user-1",
            "/emby/LiveTv/Recordings?UserId=user-1",
            "/LiveTv/Recordings?UserId=user-1",
            "/emby/LiveTv/Recordings/Groups?UserId=user-1",
            "/LiveTv/Recordings/Groups?UserId=user-1",
            "/emby/LiveTv/Timers?UserId=user-1",
            "/LiveTv/Timers?UserId=user-1",
            "/emby/LiveTv/SeriesTimers?UserId=user-1",
            "/LiveTv/SeriesTimers?UserId=user-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("live tv empty query request should succeed");

            assert_eq!(response.status(), StatusCode::OK);
            let body = body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body should read");
            let value: serde_json::Value =
                serde_json::from_slice(&body).expect("body should be json");
            assert_eq!(value["TotalRecordCount"], 0);
            assert!(value["Items"].as_array().is_some_and(Vec::is_empty));
        }

        for uri in [
            "/emby/LiveTv/ListingProviders/Default",
            "/LiveTv/ListingProviders/Default",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("live tv listing provider default request should succeed");

            assert_eq!(response.status(), StatusCode::OK);
            let body = body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body should read");
            let value: serde_json::Value =
                serde_json::from_slice(&body).expect("body should be json");
            assert_eq!(value["Type"], "None");
            assert_eq!(value["EnableAllTuners"], false);
            assert!(
                value["ChannelMappings"]
                    .as_array()
                    .is_some_and(Vec::is_empty)
            );
        }

        for uri in [
            "/emby/LiveTv/Timers/Defaults?ProgramId=program-1",
            "/LiveTv/Timers/Defaults?ProgramId=program-1",
            "/emby/LiveTv/Timers/Defaults?programId=program-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("live tv timer defaults request should succeed");

            assert_eq!(response.status(), StatusCode::OK);
            let body = body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body should read");
            let value: serde_json::Value =
                serde_json::from_slice(&body).expect("body should be json");
            assert_eq!(value["ProgramId"], "program-1");
            assert_eq!(value["TimerType"], "Program");
            assert_eq!(value["RecordAnyTime"], false);
            assert_eq!(value["RecordAnyChannel"], false);
            assert!(value["ChannelIds"].as_array().is_some_and(Vec::is_empty));
        }
    }

    #[tokio::test]
    async fn emby_live_tv_official_program_and_recording_probes_are_controlled() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri) in [
            (
                Method::GET,
                "/emby/LiveTv/Programs/Recommended?UserId=user-1",
            ),
            (Method::GET, "/LiveTv/Programs/Recommended?UserId=user-1"),
            (
                Method::POST,
                "/emby/LiveTv/Programs?UserId=user-1&StartIndex=0&Limit=10",
            ),
            (
                Method::POST,
                "/LiveTv/Programs?UserId=user-1&StartIndex=0&Limit=10",
            ),
            (Method::GET, "/emby/LiveTv/Recordings/Folders?UserId=user-1"),
            (Method::GET, "/LiveTv/Recordings/Folders?UserId=user-1"),
            (Method::GET, "/emby/LiveTv/Recordings/Series?UserId=user-1"),
            (Method::GET, "/LiveTv/Recordings/Series?UserId=user-1"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("live tv empty query request should succeed");

            assert_eq!(response.status(), StatusCode::OK);
            let body = body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body should read");
            let value: serde_json::Value =
                serde_json::from_slice(&body).expect("body should be json");
            assert_eq!(value["TotalRecordCount"], 0);
            assert!(value["Items"].as_array().is_some_and(Vec::is_empty));
        }

        for uri in [
            "/emby/LiveTv/Channels/channel-1?UserId=user-1",
            "/LiveTv/Channels/channel-1?UserId=user-1",
            "/emby/LiveTv/Recordings/recording-1?UserId=user-1",
            "/LiveTv/Recordings/recording-1?UserId=user-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("live tv detail request should succeed");

            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            let body = body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body should read");
            assert!(body.is_empty(), "live tv detail miss should be controlled");
        }
    }

    #[tokio::test]
    async fn emby_live_tv_program_detail_aliases_return_controlled_not_found() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/LiveTv/Programs/program-1?UserId=user-1",
            "/LiveTv/Programs/program-1?UserId=user-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("live tv program detail request should succeed");

            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            let body = body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body should read");
            assert!(body.is_empty(), "live tv program miss should be controlled");
        }
    }

    #[tokio::test]
    async fn emby_channels_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Channels?UserId=user-1&StartIndex=20&Limit=10",
            "/Channels?UserId=user-1&StartIndex=20&Limit=10",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("channels request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_content_service_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Users/user-1/HomeSections",
            "/Users/user-1/HomeSections",
            "/emby/Users/user-1/Sections/latestmedia/Items?Limit=12&Fields=PrimaryImageAspectRatio",
            "/Users/user-1/Sections/latestmedia/Items?Limit=12&Fields=PrimaryImageAspectRatio",
            "/emby/Users/user-1/Sections/resume/Items?StartIndex=0&Limit=12",
            "/Users/user-1/Sections/resume/Items?StartIndex=0&Limit=12",
            "/emby/Users/user-1/Sections/music/Items?parentId=library-1&startIndex=0&limit=12&includeItemTypes=Audio&mediaTypes=Audio&sortBy=SortName&sortOrder=Ascending&fields=MediaSources&enableImages=true&imageTypeLimit=1&enableImageTypes=Primary",
            "/Users/user-1/Sections/resume/Items?parent_id=library-1&start_index=0&limit=12&include_item_types=Movie&media_types=Video&sort_by=DateCreated&sort_order=Descending&enable_images=false&image_type_limit=1&enable_image_types=Primary&is_favorite=false&is_played=true",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("content service request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_display_preferences_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri, body) in [
            (
                Method::GET,
                "/emby/DisplayPreferences/item-1?UserId=user-1&Client=Infuse",
                Body::empty(),
            ),
            (
                Method::GET,
                "/emby/DisplayPreferences/item-1?userId=user-1&client=Infuse",
                Body::empty(),
            ),
            (
                Method::GET,
                "/DisplayPreferences/item-1?UserId=user-1&Client=Infuse",
                Body::empty(),
            ),
            (
                Method::POST,
                "/emby/DisplayPreferences/item-1?UserId=user-1",
                Body::from(
                    r#"{"Id":"item-1","SortBy":"DateCreated","SortOrder":"Descending","Client":"Infuse","CustomPrefs":{"view":"poster"}}"#,
                ),
            ),
            (
                Method::POST,
                "/emby/DisplayPreferences/item-1?userId=user-1&client=QueryClient",
                Body::from(
                    r#"{"id":"item-1","sortBy":"DateCreated","sortOrder":"Descending","client":"Infuse","customPrefs":{"view":"poster"}}"#,
                ),
            ),
            (
                Method::POST,
                "/DisplayPreferences/item-1?UserId=user-1",
                Body::from(
                    r#"{"Id":"item-1","SortBy":"DateCreated","SortOrder":"Descending","Client":"Infuse","CustomPrefs":{"view":"poster"}}"#,
                ),
            ),
            (Method::GET, "/emby/UserSettings/user-1", Body::empty()),
            (Method::GET, "/UserSettings/user-1", Body::empty()),
            (
                Method::POST,
                "/emby/UserSettings/user-1",
                Body::from(r#"[{"Name":"theme","Value":"dark"}]"#),
            ),
            (
                Method::POST,
                "/emby/UserSettings/user-1",
                Body::from(r#"[{"name":"theme","value":"dark"},{"key":"layout","value":"grid"}]"#),
            ),
            (
                Method::POST,
                "/UserSettings/user-1",
                Body::from(r#"[{"Name":"theme","Value":"dark"}]"#),
            ),
            (
                Method::POST,
                "/emby/UserSettings/user-1/Partial",
                Body::from(r#"{"theme":"dark"}"#),
            ),
            (
                Method::POST,
                "/UserSettings/user-1/Partial",
                Body::from(r#"{"theme":"dark"}"#),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(body)
                        .expect("request should build"),
                )
                .await
                .expect("display preferences request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_user_typed_settings_and_track_selection_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri, body) in [
            (
                Method::GET,
                "/emby/Users/user-1/TypedSettings/playback.audio",
                Body::empty(),
            ),
            (
                Method::GET,
                "/Users/user-1/TypedSettings/playback.audio",
                Body::empty(),
            ),
            (
                Method::POST,
                "/emby/Users/user-1/TypedSettings/playback.audio",
                Body::from(r#"{"enabled":true}"#),
            ),
            (
                Method::POST,
                "/Users/user-1/TypedSettings/playback.audio",
                Body::from(r#"{"enabled":true}"#),
            ),
            (
                Method::DELETE,
                "/emby/Users/user-1/TrackSelections/Audio",
                Body::empty(),
            ),
            (
                Method::DELETE,
                "/Users/user-1/TrackSelections/Audio",
                Body::empty(),
            ),
            (
                Method::POST,
                "/emby/Users/user-1/TrackSelections/Subtitle/Delete",
                Body::empty(),
            ),
            (
                Method::POST,
                "/Users/user-1/TrackSelections/Subtitle/Delete",
                Body::empty(),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(body)
                        .expect("request should build"),
                )
                .await
                .expect("user settings request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        }
    }

    #[tokio::test]
    async fn admin_library_routes_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        let cases = [
            (Method::POST, "/api/admin/libraries"),
            (Method::POST, "/api/admin/libraries/library-1/paths"),
            (Method::POST, "/api/admin/libraries/library-1/scan"),
            (
                Method::POST,
                "/api/admin/libraries/library-1/metadata/refresh",
            ),
            (
                Method::POST,
                "/api/admin/media-items/item-1/metadata/refresh",
            ),
            (Method::GET, "/api/admin/metadata/providers"),
            (Method::GET, "/api/admin/users"),
            (
                Method::PUT,
                "/api/admin/users/00000000-0000-0000-0000-000000000001/policy",
            ),
            (
                Method::GET,
                "/api/admin/users/00000000-0000-0000-0000-000000000001/libraries",
            ),
            (
                Method::PUT,
                "/api/admin/users/00000000-0000-0000-0000-000000000001/libraries/00000000-0000-0000-0000-000000000002/permissions",
            ),
            (Method::GET, "/api/admin/jobs"),
            (
                Method::GET,
                "/api/admin/jobs/00000000-0000-0000-0000-000000000001",
            ),
            (
                Method::GET,
                "/api/admin/jobs/00000000-0000-0000-0000-000000000001/runs",
            ),
            (
                Method::GET,
                "/api/admin/jobs/00000000-0000-0000-0000-000000000001/events",
            ),
            (Method::POST, "/api/admin/jobs/job-1/run"),
            (Method::GET, "/api/admin/scheduled-tasks"),
            (
                Method::GET,
                "/api/admin/scheduled-tasks/core.library.incremental_scan/runs",
            ),
            (
                Method::POST,
                "/api/admin/scheduled-tasks/core.library.incremental_scan/run",
            ),
            (Method::GET, "/api/admin/transcoding-sessions"),
            (
                Method::POST,
                "/api/admin/transcoding-sessions/session-1/cancel",
            ),
            (Method::GET, "/api/admin/plugins"),
            (Method::GET, "/api/admin/plugins/capabilities"),
            (Method::GET, "/api/admin/plugins/menu-items"),
            (Method::GET, "/api/admin/plugins/packages"),
            (Method::POST, "/api/admin/plugins/packages"),
            (Method::GET, "/api/admin/plugins/packages/package-1"),
            (
                Method::POST,
                "/api/admin/plugins/packages/package-1/approve",
            ),
            (Method::POST, "/api/admin/plugins/packages/package-1/reject"),
            (
                Method::POST,
                "/api/admin/plugins/packages/package-1/activate",
            ),
            (Method::POST, "/api/admin/plugins/dev.fbz.test/enable"),
            (Method::POST, "/api/admin/plugins/dev.fbz.test/disable"),
            (Method::GET, "/api/admin/plugins/dev.fbz.test/config"),
            (Method::PUT, "/api/admin/plugins/dev.fbz.test/config"),
            (Method::GET, "/api/admin/notification-targets"),
            (Method::POST, "/api/admin/notification-targets"),
            (Method::PUT, "/api/admin/notification-targets/target-1"),
            (
                Method::POST,
                "/api/admin/notification-targets/target-1/enable",
            ),
            (
                Method::POST,
                "/api/admin/notification-targets/target-1/disable",
            ),
            (Method::GET, "/api/admin/notification-requests"),
            (
                Method::GET,
                "/api/admin/notification-requests/request-1/attempts",
            ),
            (
                Method::POST,
                "/api/admin/notification-requests/request-1/retry",
            ),
            (Method::GET, "/api/admin/plugin-dispatches"),
            (Method::GET, "/api/admin/plugin-dispatches/dispatch-1/runs"),
            (Method::GET, "/api/admin/plugin-host-api-calls"),
            (
                Method::GET,
                "/api/admin/plugin-execution-runs/run-1/host-api-calls",
            ),
            (Method::GET, "/api/admin/event-stream-mirror/status"),
            (
                Method::POST,
                "/api/admin/plugin-dispatches/dispatch-1/replay",
            ),
            (Method::GET, "/api/plugin/config"),
        ];

        for (method, uri) in cases {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(Body::from("{}"))
                        .expect("request should build"),
                )
                .await
                .expect("admin request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_public_users_aliases_return_ok() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in ["/emby/Users/Public", "/Users/Public"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("public users request should succeed");

            assert_eq!(response.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn emby_user_detail_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Users/Me",
            "/Users/Me",
            "/emby/Users/user-1",
            "/Users/user-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("user detail request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_authenticate_route_exists() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Users/AuthenticateByName",
            "/Users/AuthenticateByName",
            "/emby/Users/user-1/Authenticate",
            "/Users/user-1/Authenticate",
        ] {
            let body = if uri.ends_with("/AuthenticateByName") {
                r#"{"Username":"admin","Pw":"secret"}"#
            } else {
                r#"{"Pw":"secret"}"#
            };
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header(
                            "authorization",
                            r#"Emby Client="Test", Device="Test Device", DeviceId="test-device", Version="1.0""#,
                        )
                        .body(Body::from(body))
                        .expect("request should build"),
                )
                .await
                .expect("authenticate request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        }
    }

    #[tokio::test]
    async fn emby_user_password_recovery_and_prefix_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri, body) in [
            (
                Method::POST,
                "/emby/Users/ForgotPassword",
                Body::from(r#"{"EnteredUsername":"alice"}"#),
            ),
            (
                Method::POST,
                "/emby/Users/ForgotPassword",
                Body::from(r#"{"enteredUsername":"alice"}"#),
            ),
            (
                Method::POST,
                "/Users/ForgotPassword",
                Body::from(r#"{"entered_username":"alice"}"#),
            ),
            (
                Method::POST,
                "/Users/ForgotPassword",
                Body::from(r#"{"EnteredUsername":"alice"}"#),
            ),
            (
                Method::POST,
                "/emby/Users/ForgotPassword/Pin",
                Body::from(r#"{"Pin":"123456"}"#),
            ),
            (
                Method::POST,
                "/emby/Users/ForgotPassword/Pin",
                Body::from(r#"{"pin":"123456"}"#),
            ),
            (
                Method::POST,
                "/Users/ForgotPassword/Pin",
                Body::from(r#"{"Pin":"123456"}"#),
            ),
            (
                Method::GET,
                "/emby/Users/Prefixes?IsDisabled=false&Limit=20&SortOrder=Ascending",
                Body::empty(),
            ),
            (
                Method::GET,
                "/Users/Prefixes?IsDisabled=false&Limit=20&SortOrder=Ascending",
                Body::empty(),
            ),
            (
                Method::GET,
                "/emby/Users/Prefixes?isDisabled=false&startIndex=0&limit=20&sortOrder=Ascending",
                Body::empty(),
            ),
            (
                Method::GET,
                "/Users/Prefixes?is_disabled=false&start_index=0&limit=20&name_starts_with_or_greater=A&sort_order=Ascending",
                Body::empty(),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(body)
                        .expect("request should build"),
                )
                .await
                .expect("user password recovery request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_user_management_write_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri, body) in [
            (
                Method::POST,
                "/emby/Users/New",
                Body::from(r#"{"Name":"alice"}"#),
            ),
            (
                Method::POST,
                "/emby/Users/New",
                Body::from(
                    r#"{"name":"alice","copyFromUserId":"template","userCopyOptions":["UserPolicy"]}"#,
                ),
            ),
            (
                Method::POST,
                "/Users/New",
                Body::from(
                    r#"{"name":"alice","copy_from_user_id":"template","user_copy_options":["UserConfiguration"]}"#,
                ),
            ),
            (
                Method::POST,
                "/Users/New",
                Body::from(r#"{"Name":"alice"}"#),
            ),
            (
                Method::POST,
                "/emby/Users/user-1",
                Body::from(r#"{"Id":"user-1","Name":"Alice"}"#),
            ),
            (
                Method::POST,
                "/Users/user-1",
                Body::from(r#"{"Id":"user-1","Name":"Alice"}"#),
            ),
            (Method::DELETE, "/emby/Users/user-1", Body::empty()),
            (Method::DELETE, "/Users/user-1", Body::empty()),
            (Method::POST, "/emby/Users/user-1/Delete", Body::empty()),
            (Method::POST, "/Users/user-1/Delete", Body::empty()),
            (
                Method::POST,
                "/emby/Users/user-1/Configuration",
                Body::from(r#"{"AudioLanguagePreference":"eng"}"#),
            ),
            (
                Method::POST,
                "/Users/user-1/Configuration",
                Body::from(r#"{"AudioLanguagePreference":"eng"}"#),
            ),
            (
                Method::POST,
                "/emby/Users/user-1/Configuration/Partial",
                Body::from(r#"{"RememberAudioSelections":true}"#),
            ),
            (
                Method::POST,
                "/emby/Users/user-1/Configuration/Partial",
                Body::from(r#"{"rememberAudioSelections":true,"resumeRewindSeconds":5}"#),
            ),
            (
                Method::POST,
                "/Users/user-1/Configuration/Partial",
                Body::from(r#"{"RememberAudioSelections":true}"#),
            ),
            (
                Method::POST,
                "/Users/user-1/Configuration/Partial",
                Body::from(r#"{"remember_audio_selections":true,"resume_rewind_seconds":5}"#),
            ),
            (
                Method::POST,
                "/emby/Users/user-1/Policy",
                Body::from(r#"{"IsAdministrator":false}"#),
            ),
            (
                Method::POST,
                "/emby/Users/user-1/Policy",
                Body::from(r#"{"enableContentDownloading":true,"maxParentalRating":100}"#),
            ),
            (
                Method::POST,
                "/Users/user-1/Policy",
                Body::from(r#"{"IsAdministrator":false}"#),
            ),
            (
                Method::POST,
                "/Users/user-1/Policy",
                Body::from(r#"{"enable_content_downloading":true,"max_parental_rating":100}"#),
            ),
            (
                Method::POST,
                "/emby/Users/user-1/Password",
                Body::from(r#"{"Id":"user-1","NewPw":"secret","ResetPassword":false}"#),
            ),
            (
                Method::POST,
                "/emby/Users/user-1/EasyPassword",
                Body::from(r#"{"Id":"user-1","NewPw":"1234","ResetPassword":false}"#),
            ),
            (
                Method::POST,
                "/emby/Users/user-1/EasyPassword",
                Body::from(r#"{"id":"user-1","newPw":"1234","resetPassword":false}"#),
            ),
            (
                Method::POST,
                "/Users/user-1/EasyPassword",
                Body::from(r#"{"id":"user-1","new_pw":"1234","reset_password":false}"#),
            ),
            (
                Method::POST,
                "/emby/Users/user-1/Password",
                Body::from(r#"{"id":"user-1","newPw":"secret","resetPassword":false}"#),
            ),
            (
                Method::POST,
                "/Users/user-1/Password",
                Body::from(r#"{"id":"user-1","new_pw":"secret","reset_password":false}"#),
            ),
            (
                Method::POST,
                "/Users/user-1/Password",
                Body::from(r#"{"Id":"user-1","NewPw":"secret","ResetPassword":false}"#),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(body)
                        .expect("request should build"),
                )
                .await
                .expect("user management write request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_connect_service_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri, body) in [
            (
                Method::GET,
                "/emby/Connect/Exchange?ConnectUserName=alice&Pin=123456",
                Body::empty(),
            ),
            (
                Method::GET,
                "/Connect/Exchange?connectUserName=alice&pin=123456",
                Body::empty(),
            ),
            (
                Method::GET,
                "/emby/Connect/Pending?ConnectUserName=alice",
                Body::empty(),
            ),
            (
                Method::GET,
                "/Connect/Pending?connect_user_name=alice",
                Body::empty(),
            ),
            (
                Method::POST,
                "/emby/Users/user-1/Connect/Link",
                Body::from(r#"{"ConnectUserName":"alice"}"#),
            ),
            (
                Method::POST,
                "/Users/user-1/Connect/Link",
                Body::from(r#"{"connectUserName":"alice"}"#),
            ),
            (
                Method::DELETE,
                "/emby/Users/user-1/Connect/Link",
                Body::empty(),
            ),
            (
                Method::POST,
                "/Users/user-1/Connect/Link/Delete",
                Body::empty(),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(body)
                        .expect("request should build"),
                )
                .await
                .expect("connect service request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND, "{uri}");
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED, "{uri}");
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
        }
    }

    #[tokio::test]
    async fn emby_logout_route_exists() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri) in [
            (Method::GET, "/emby/Users/Query"),
            (Method::GET, "/Users/Query"),
            (
                Method::GET,
                "/emby/Users/Query?isHidden=false&isDisabled=true&startIndex=0&limit=20&nameStartsWithOrGreater=A&sortOrder=Descending",
            ),
            (
                Method::GET,
                "/Users/Query?is_hidden=false&is_disabled=true&start_index=0&limit=20&name_starts_with_or_greater=A&sort_order=Descending",
            ),
            (Method::GET, "/emby/Users/ItemAccess"),
            (Method::GET, "/Users/ItemAccess"),
            (Method::GET, "/emby/Auth/Providers"),
            (Method::GET, "/Auth/Providers"),
            (Method::GET, "/emby/Auth/Keys?StartIndex=0&Limit=20"),
            (Method::GET, "/Auth/Keys?StartIndex=0&Limit=20"),
            (Method::POST, "/emby/Auth/Keys?App=TestClient"),
            (Method::POST, "/Auth/Keys?App=TestClient"),
            (Method::DELETE, "/emby/Auth/Keys/test-key"),
            (Method::DELETE, "/Auth/Keys/test-key"),
            (Method::POST, "/emby/Auth/Keys/test-key/Delete"),
            (Method::POST, "/Auth/Keys/test-key/Delete"),
            (Method::GET, "/emby/Sessions"),
            (Method::GET, "/Sessions"),
            (Method::POST, "/emby/Sessions/Logout"),
            (Method::POST, "/Sessions/Logout"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("logout request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[test]
    fn emby_user_item_access_routes_are_static_aliases() {
        let routes = include_str!("compat/emby/routes/mod.rs");

        assert!(routes.contains("\"/emby/Users/ItemAccess\""));
        assert!(routes.contains("\"/Users/ItemAccess\""));
    }

    #[tokio::test]
    async fn emby_session_capability_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (uri, body) in [
            (
                "/emby/Sessions/Capabilities?Id=00000000-0000-0000-0000-000000000001&PlayableMediaTypes=Audio,Video&SupportedCommands=Play,Pause&SupportsMediaControl=true",
                Body::empty(),
            ),
            (
                "/Sessions/Capabilities?Id=00000000-0000-0000-0000-000000000001&PlayableMediaTypes=Audio,Video",
                Body::empty(),
            ),
            (
                "/emby/Sessions/Capabilities/Full?Id=00000000-0000-0000-0000-000000000001",
                Body::from(
                    r#"{"PlayableMediaTypes":["Audio","Video"],"SupportedCommands":["Play"],"SupportsMediaControl":true}"#,
                ),
            ),
            (
                "/Sessions/Capabilities/Full?Id=00000000-0000-0000-0000-000000000001",
                Body::from(
                    r#"{"PlayableMediaTypes":["Audio","Video"],"SupportedCommands":["Play"],"SupportsMediaControl":true}"#,
                ),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(body)
                        .expect("request should build"),
                )
                .await
                .expect("session capability request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_session_capabilities_accept_lower_camel_fields() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (uri, body) in [
            (
                "/emby/Sessions/Capabilities?id=00000000-0000-0000-0000-000000000001&playableMediaTypes=Audio,Video&supportedCommands=Play,Pause&supportsMediaControl=true&supportsSync=true",
                Body::empty(),
            ),
            (
                "/Sessions/Capabilities/Full?id=00000000-0000-0000-0000-000000000001",
                Body::from(
                    r#"{"playableMediaTypes":["Audio","Video"],"supportedCommands":["Play"],"supportsMediaControl":true,"supportsSync":true,"pushToken":"token","pushTokenType":"fcm","deviceProfile":{"Name":"Client"},"iconUrl":"https://example.test/icon.png","appId":"app.id"}"#,
                ),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(body)
                        .expect("request should build"),
                )
                .await
                .expect("session capability request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_session_detail_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Sessions?UserId=user-1",
            "/emby/Sessions?userId=user-1",
            "/emby/Sessions?user_id=user-1",
            "/Sessions?UserId=user-1",
            "/emby/Sessions/00000000-0000-0000-0000-000000000001",
            "/Sessions/00000000-0000-0000-0000-000000000001",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("session detail request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_session_remote_control_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Sessions/00000000-0000-0000-0000-000000000001/Playing",
            "/Sessions/00000000-0000-0000-0000-000000000001/Playing",
            "/emby/Sessions/00000000-0000-0000-0000-000000000001/Playing/Pause",
            "/Sessions/00000000-0000-0000-0000-000000000001/Playing/Seek?SeekPositionTicks=42",
            "/emby/Sessions/00000000-0000-0000-0000-000000000001/Command/DisplayMessage",
            "/Sessions/00000000-0000-0000-0000-000000000001/Command",
            "/emby/Sessions/Command/DisplayMessage",
            "/Sessions/Command/DisplayMessage",
            "/emby/Sessions/00000000-0000-0000-0000-000000000001/System/GoHome",
            "/Sessions/00000000-0000-0000-0000-000000000001/Message",
            "/emby/Sessions/00000000-0000-0000-0000-000000000001/Viewing",
            "/emby/Sessions/00000000-0000-0000-0000-000000000001/Users/00000000-0000-0000-0000-000000000002",
            "/Sessions/00000000-0000-0000-0000-000000000001/Users/00000000-0000-0000-0000-000000000002",
            "/emby/Sessions/00000000-0000-0000-0000-000000000001/Users/00000000-0000-0000-0000-000000000002/Delete",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(Body::from(
                            r#"{"ItemIds":["item-1"],"PlayCommand":"PlayNow"}"#,
                        ))
                        .expect("request should build"),
                )
                .await
                .expect("session remote control request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/Sessions/00000000-0000-0000-0000-000000000001/Users/00000000-0000-0000-0000-000000000002")
                    .header("content-type", "application/json")
                    .header("x-emby-token", "test-token")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("session user delete request should succeed");

        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn emby_session_remote_control_body_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (uri, body) in [
            (
                "/emby/Sessions/00000000-0000-0000-0000-000000000001/Command",
                r#"{"command":"DisplayMessage","arguments":{"Header":"Now playing"}}"#,
            ),
            (
                "/emby/Sessions/Command/DisplayMessage",
                r#"{"name":"DisplayMessage","arguments":{"Header":"Now playing"}}"#,
            ),
            (
                "/emby/Sessions/00000000-0000-0000-0000-000000000001/Message",
                r#"{"header":"Playback","text":"Paused","timeoutMs":1500}"#,
            ),
            (
                "/emby/Sessions/00000000-0000-0000-0000-000000000001/Viewing",
                r#"{"itemId":"item-1","itemName":"Movie","itemType":"Movie"}"#,
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(Body::from(body))
                        .expect("request should build"),
                )
                .await
                .expect("session remote control body alias request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_session_remote_play_accepts_lower_camel_fields() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (uri, body) in [
            (
                "/emby/Sessions/00000000-0000-0000-0000-000000000001/Playing",
                Body::from(
                    r#"{"itemIds":["item-1"],"playCommand":"PlayNow","startPositionTicks":42,"mediaSourceId":"source-1"}"#,
                ),
            ),
            (
                "/Sessions/00000000-0000-0000-0000-000000000001/Playing?itemIds=item-1,item-2&playCommand=PlayNext&startPositionTicks=42&mediaSourceId=source-1",
                Body::empty(),
            ),
            (
                "/Sessions/00000000-0000-0000-0000-000000000001/Playing/Seek?seekPositionTicks=42",
                Body::empty(),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(body)
                        .expect("request should build"),
                )
                .await
                .expect("session remote play request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_session_play_queue_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Sessions/PlayQueue?Id=00000000-0000-0000-0000-000000000001&DeviceId=device-1",
            "/emby/Sessions/PlayQueue?id=00000000-0000-0000-0000-000000000001&deviceId=device-1",
            "/emby/Sessions/PlayQueue?id=00000000-0000-0000-0000-000000000001&device_id=device-1",
            "/Sessions/PlayQueue?Id=00000000-0000-0000-0000-000000000001&DeviceId=device-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("session play queue request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_device_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri, body) in [
            (Method::GET, "/emby/Devices", Body::empty()),
            (Method::GET, "/Devices", Body::empty()),
            (
                Method::GET,
                "/emby/Devices?sortOrder=Ascending",
                Body::empty(),
            ),
            (Method::DELETE, "/emby/Devices?Id=device-1", Body::empty()),
            (Method::DELETE, "/Devices?Id=device-1", Body::empty()),
            (Method::DELETE, "/emby/Devices?id=device-1", Body::empty()),
            (Method::GET, "/emby/Devices/Info?Id=device-1", Body::empty()),
            (Method::GET, "/Devices/Info?Id=device-1", Body::empty()),
            (Method::GET, "/emby/Devices/Info?id=device-1", Body::empty()),
            (Method::GET, "/emby/Devices/CameraUploads", Body::empty()),
            (Method::GET, "/Devices/CameraUploads", Body::empty()),
            (
                Method::POST,
                "/emby/Devices/CameraUploads?Album=Camera&Name=photo.jpg&Id=file-1",
                Body::empty(),
            ),
            (
                Method::POST,
                "/Devices/CameraUploads?Album=Camera&Name=photo.jpg&Id=file-1",
                Body::empty(),
            ),
            (
                Method::POST,
                "/emby/Devices/Delete?Id=device-1",
                Body::empty(),
            ),
            (Method::POST, "/Devices/Delete?Id=device-1", Body::empty()),
            (
                Method::POST,
                "/emby/Devices/Delete?id=device-1",
                Body::empty(),
            ),
            (
                Method::GET,
                "/emby/Devices/Options?Id=device-1",
                Body::empty(),
            ),
            (Method::GET, "/Devices/Options?Id=device-1", Body::empty()),
            (
                Method::GET,
                "/emby/Devices/Options?id=device-1",
                Body::empty(),
            ),
            (
                Method::POST,
                "/emby/Devices/Options?Id=device-1",
                Body::from(r#"{"CustomName":"Living Room"}"#),
            ),
            (
                Method::POST,
                "/Devices/Options?Id=device-1",
                Body::from(r#"{"CustomName":"Living Room"}"#),
            ),
            (
                Method::POST,
                "/emby/Devices/Options?id=device-1",
                Body::from(r#"{"CustomName":"Living Room"}"#),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(body)
                        .expect("request should build"),
                )
                .await
                .expect("device request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_scheduled_task_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri) in [
            (Method::GET, "/emby/ScheduledTasks"),
            (Method::GET, "/ScheduledTasks"),
            (
                Method::GET,
                "/emby/ScheduledTasks/core.library.incremental_scan",
            ),
            (Method::GET, "/ScheduledTasks/core.library.incremental_scan"),
            (
                Method::GET,
                "/emby/ScheduledTasks?IsEnabled=true&IsHidden=false",
            ),
            (
                Method::GET,
                "/emby/ScheduledTasks?isEnabled=true&isHidden=false",
            ),
            (
                Method::GET,
                "/ScheduledTasks?is_enabled=false&is_hidden=false",
            ),
            (Method::GET, "/ScheduledTasks?IsEnabled=false"),
            (
                Method::POST,
                "/emby/ScheduledTasks/Running/core.library.incremental_scan",
            ),
            (
                Method::POST,
                "/ScheduledTasks/Running/core.library.incremental_scan",
            ),
            (
                Method::DELETE,
                "/emby/ScheduledTasks/Running/core.library.incremental_scan",
            ),
            (
                Method::DELETE,
                "/ScheduledTasks/Running/core.library.incremental_scan",
            ),
            (
                Method::POST,
                "/emby/ScheduledTasks/Running/core.library.incremental_scan/Delete",
            ),
            (
                Method::POST,
                "/ScheduledTasks/Running/core.library.incremental_scan/Delete",
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("scheduled task request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_user_views_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Users/user-1/Views",
            "/Users/user-1/Views",
            "/emby/Users/user-1/GroupingOptions",
            "/Users/user-1/GroupingOptions",
            "/emby/UserViews?UserId=user-1",
            "/UserViews?UserId=user-1",
            "/emby/UserViews?userId=user-1",
        ] {
            let mut builder = Request::builder().method(Method::GET).uri(uri);
            if !uri.contains("GroupingOptions") {
                builder = builder.header("x-emby-token", "test-token");
            }

            let response = app
                .clone()
                .oneshot(builder.body(Body::empty()).expect("request should build"))
                .await
                .expect("views request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_library_media_folder_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Library/MediaFolders",
            "/Library/MediaFolders",
            "/emby/Library/MediaFolders?isHidden=false",
            "/emby/Library/SelectableMediaFolders",
            "/Library/SelectableMediaFolders",
            "/emby/Library/VirtualFolders",
            "/Library/VirtualFolders",
            "/emby/Library/VirtualFolders/Query?StartIndex=0&Limit=20",
            "/Library/VirtualFolders/Query?StartIndex=0&Limit=20",
            "/emby/Library/VirtualFolders/Query?startIndex=0&limit=20",
            "/emby/Library/PhysicalPaths",
            "/Library/PhysicalPaths",
            "/emby/Libraries/AvailableOptions",
            "/Libraries/AvailableOptions",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("library media folder request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_library_refresh_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in ["/emby/Library/Refresh", "/Library/Refresh"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("library refresh request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        }
    }

    #[tokio::test]
    async fn emby_user_items_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Users/user-1/Items",
            "/Users/user-1/Items",
            "/emby/Users/user-1/Items?ParentId=root",
            "/Users/user-1/Items?ParentId=root",
            "/emby/Users/user-1/Items?Genres=Action|Drama&PersonIds=person-1&PersonTypes=Actor&ArtistIds=artist-1",
            "/Users/user-1/Items?Genres=Action|Drama&PersonIds=person-1&PersonTypes=Actor&ArtistIds=artist-1",
            "/emby/Users/user-1/Items?Ids=item-1,item-2&ExcludeItemIds=item-3&Years=2024,2025&SearchTerm=movie&NameStartsWith=A&NameLessThan=Z",
            "/Users/user-1/Items?Ids=item-1,item-2&ExcludeItemIds=item-3&Years=2024,2025&SearchTerm=movie&NameStartsWith=A&NameLessThan=Z",
            "/emby/Users/user-1/Items?Filters=IsFavorite,IsResumable&IsPlayed=false&IsFavorite=true",
            "/Users/user-1/Items?Filters=IsFavorite,IsResumable&IsPlayed=false&IsFavorite=true",
            "/emby/Users/user-1/Items?Filters=IsFolder,IsNotFolder&IsFolder=true&IsMovie=false&IsSeries=true",
            "/Users/user-1/Items?Filters=IsFolder,IsNotFolder&IsFolder=true&IsMovie=false&IsSeries=true",
            "/emby/Users/user-1/Items?MediaTypes=Video,Audio&Containers=mp4,mkv&AudioCodecs=aac&VideoCodecs=h264&SubtitleCodecs=srt",
            "/Users/user-1/Items?MediaTypes=Video,Audio&Containers=mp4,mkv&AudioCodecs=aac&VideoCodecs=h264&SubtitleCodecs=srt",
            "/emby/Users/user-1/Items?OfficialRatings=PG-13|TV-MA&Tags=HDR|IMAX&ExcludeTags=Blocked&Studios=Studio%20A|Studio%20B&StudioIds=studio-1|studio-2",
            "/Users/user-1/Items?OfficialRatings=PG-13|TV-MA&Tags=HDR|IMAX&ExcludeTags=Blocked&Studios=Studio%20A|Studio%20B&StudioIds=studio-1|studio-2",
            "/emby/Users/user-1/Items?AnyProviderIdEquals=tmdb.123,imdb.tt7654321,tvdb.456",
            "/Users/user-1/Items?AnyProviderIdEquals=tmdb.123,imdb.tt7654321,tvdb.456",
            "/emby/Users/user-1/Items?ImageTypes=Primary,Backdrop&EnableImages=true&ImageTypeLimit=2&EnableImageTypes=Primary,Backdrop,Logo",
            "/Users/user-1/Items?ImageTypes=Primary,Backdrop&EnableImages=true&ImageTypeLimit=2&EnableImageTypes=Primary,Backdrop,Logo",
            "/emby/Users/user-1/Items?parentId=library-1&startIndex=0&limit=20&recursive=true&includeItemTypes=Movie,Episode&imageTypes=Primary,Backdrop&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary,Backdrop&anyProviderIdEquals=tmdb.123&sortBy=DateCreated&sortOrder=Descending&fields=MediaSources&filters=IsPlayed&isPlayed=true&isFavorite=false&isFolder=false&isMovie=true&isSeries=false&ids=item-1,item-2&excludeItemIds=item-3&searchTerm=movie&nameStartsWith=A&nameStartsWithOrGreater=B&nameLessThan=Z&genreIds=1,2&officialRatings=PG-13&excludeTags=Blocked&studioIds=studio-1&personIds=person-1&personTypes=Actor&artistIds=artist-1&albumIds=album-1&mediaTypes=Video&audioCodecs=aac&videoCodecs=h264&subtitleCodecs=srt",
            "/emby/Users/user-1/Items?IncludeItemTypes=Playlist&SearchTerm=mix&SortOrder=Descending",
            "/Users/user-1/Items?IncludeItemTypes=Playlist&SearchTerm=mix&SortOrder=Descending",
            "/emby/Search/Hints?UserId=user-1&SearchTerm=alien&IncludeItemTypes=Movie,Series&Limit=10",
            "/emby/Search/Hints?userId=user-1&parentId=library-1&searchTerm=alien&includeItemTypes=Movie,Series&mediaTypes=Video&startIndex=0&limit=10",
            "/Search/Hints?UserId=user-1&SearchTerm=alien&IncludeItemTypes=Movie,Series&Limit=10",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("items request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_resume_and_latest_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Users/user-1/Items/Resume",
            "/Users/user-1/Items/Resume",
            "/emby/Users/user-1/Items/Latest",
            "/Users/user-1/Items/Latest",
            "/emby/Users/user-1/Suggestions?ItemLimit=12&MediaTypes=Video",
            "/emby/Users/user-1/Suggestions?parentId=library-1&startIndex=1&limit=20&itemLimit=12&includeItemTypes=Movie,Episode&mediaTypes=Video&sortBy=DateCreated&sortOrder=Descending&fields=MediaSources",
            "/Users/user-1/Suggestions?Limit=12&IncludeItemTypes=Movie,Episode",
            "/emby/Users/user-1/Items/Counts",
            "/Users/user-1/Items/Counts",
            "/emby/Users/user-1/Items/Root",
            "/Users/user-1/Items/Root",
            "/emby/Items/Counts?UserId=user-1",
            "/emby/Items/Counts?userId=user-1",
            "/Items/Counts?UserId=user-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("media list request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_trailers_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Trailers?UserId=user-1&Limit=12&Fields=MediaSources,PrimaryImageAspectRatio",
            "/emby/Trailers?userId=user-1&parentId=library-1&startIndex=1&limit=12&recursive=false&searchTerm=signal&sortBy=DateCreated&sortOrder=Descending&fields=MediaSources&includeItemTypes=Trailer&mediaTypes=Video&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary",
            "/Trailers?UserId=user-1&Limit=12&Fields=MediaSources,PrimaryImageAspectRatio",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("trailers request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_sync_service_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Sync/Options?UserId=user-1&ItemIds=item-1&TargetId=device-1",
            "/Sync/Options?UserId=user-1&ItemIds=item-1&TargetId=device-1",
            "/emby/Sync/Targets?UserId=user-1",
            "/Sync/Targets?UserId=user-1",
            "/emby/Sync/Jobs?StartIndex=5&Limit=10",
            "/Sync/Jobs?StartIndex=5&Limit=10",
            "/emby/Sync/JobItems?TargetId=device-1",
            "/Sync/JobItems?TargetId=device-1",
            "/emby/Sync/Items/Ready?TargetId=device-1",
            "/Sync/Items/Ready?TargetId=device-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("sync service request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_sync_service_write_boundaries_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri) in [
            (
                Method::DELETE,
                "/emby/Sync/device-1/Items?ItemIds=item-1,item-2",
            ),
            (
                Method::POST,
                "/Sync/device-1/Items/Delete?ItemIds=item-1,item-2",
            ),
            (
                Method::POST,
                "/emby/Sync/Items/Cancel?ItemIds=item-1,item-2",
            ),
            (Method::DELETE, "/Sync/Jobs/job-1"),
            (Method::POST, "/emby/Sync/Jobs/job-1/Delete"),
            (Method::DELETE, "/emby/Sync/JobItems/job-item-1"),
            (Method::POST, "/Sync/JobItems/job-item-1/Delete"),
            (Method::POST, "/emby/Sync/JobItems/job-item-1/Enable"),
            (Method::POST, "/Sync/JobItems/job-item-1/MarkForRemoval"),
            (
                Method::POST,
                "/emby/Sync/JobItems/job-item-1/UnmarkForRemoval",
            ),
            (Method::POST, "/Sync/JobItems/job-item-1/Transferred"),
            (Method::POST, "/emby/Sync/item-1/Status"),
            (Method::POST, "/Sync/Data?TargetId=device-1"),
            (Method::POST, "/emby/Sync/OfflineActions"),
            (Method::POST, "/Sync/Jobs"),
            (Method::POST, "/emby/Sync/Jobs/job-1"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("sync service write request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND, "uri {uri}");
        }
    }

    #[tokio::test]
    async fn emby_sync_service_file_probe_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri) in [
            (Method::GET, "/emby/Sync/Jobs/job-1"),
            (Method::GET, "/Sync/Jobs/job-1"),
            (
                Method::GET,
                "/emby/Sync/JobItems/job-item-1/AdditionalFiles?Name=poster.jpg",
            ),
            (
                Method::GET,
                "/Sync/JobItems/job-item-1/AdditionalFiles?Name=poster.jpg",
            ),
            (Method::GET, "/emby/Sync/JobItems/job-item-1/File"),
            (Method::GET, "/Sync/JobItems/job-item-1/File"),
            (Method::HEAD, "/emby/Sync/JobItems/job-item-1/File"),
            (Method::HEAD, "/Sync/JobItems/job-item-1/File"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("sync service file probe request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND, "uri {uri}");
            assert_ne!(
                response.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "uri {uri}"
            );
        }
    }

    #[tokio::test]
    async fn emby_show_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Shows/NextUp",
            "/Shows/NextUp",
            "/emby/Shows/NextUp?userId=user-1&seriesId=series-1&parentId=library-1&startIndex=1&limit=20&includeItemTypes=Episode&sortBy=IndexNumber&sortOrder=Descending&fields=Overview",
            "/emby/Shows/Upcoming?userId=user-1&parentId=library-1&startIndex=1&limit=20&includeItemTypes=Episode&sortBy=PremiereDate&sortOrder=Ascending&fields=Overview",
            "/emby/Shows/series-1/Seasons",
            "/Shows/series-1/Seasons",
            "/emby/Shows/series-1/Seasons?userId=user-1&startIndex=1&limit=20&includeItemTypes=Season&sortBy=SortName&sortOrder=Ascending&fields=Overview",
            "/emby/Shows/series-1/Episodes",
            "/Shows/series-1/Episodes",
            "/emby/Shows/series-1/Episodes?userId=user-1&seasonId=season-1&startIndex=1&limit=20&includeItemTypes=Episode&sortBy=IndexNumber&sortOrder=Ascending&fields=Overview",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("show request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_genre_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Genres?UserId=user-1&Limit=20",
            "/Genres?UserId=user-1&Limit=20",
            "/emby/Genres?userId=user-1&parentId=library-1&startIndex=1&limit=20&searchTerm=act&sortOrder=Descending&nameStartsWith=A&nameStartsWithOrGreater=A&fields=PrimaryImageAspectRatio&enableImages=true",
            "/emby/Genres/Action?UserId=user-1",
            "/Genres/Action?UserId=user-1",
            "/emby/Genres/Action?userId=user-1",
            "/emby/MusicGenres?UserId=user-1&SearchTerm=rock",
            "/MusicGenres?UserId=user-1&SearchTerm=rock",
            "/emby/MusicGenres?userId=user-1&searchTerm=rock",
            "/emby/MusicGenres/Rock?UserId=user-1",
            "/MusicGenres/Rock?UserId=user-1",
            "/emby/MusicGenres/Rock?userId=user-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("genre request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_artist_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Artists?UserId=user-1&Limit=20",
            "/Artists?UserId=user-1&Limit=20",
            "/emby/Artists?userId=user-1&parentId=library-1&startIndex=1&limit=20&searchTerm=bow&sortOrder=Descending&artistType=AlbumArtist&nameStartsWith=B&artistStartsWithOrGreater=A&albumArtistStartsWithOrGreater=C&albums=Low%7CHeroes&albumIds=album-1&fields=PrimaryImageAspectRatio&enableImages=true",
            "/emby/Artists/AlbumArtists?UserId=user-1&SearchTerm=bow",
            "/Artists/AlbumArtists?UserId=user-1&SearchTerm=bow",
            "/emby/Artists/AlbumArtists?userId=user-1&searchTerm=bow",
            "/emby/Artists/Prefixes?UserId=user-1&Limit=20",
            "/Artists/Prefixes?UserId=user-1&Limit=20",
            "/emby/Artists/Prefixes?userId=user-1&parentId=library-1&startIndex=1&limit=20&searchTerm=bow&nameStartsWith=B&nameStartsWithOrGreater=A&nameLessThan=Z&artistType=AlbumArtist",
            "/emby/Artists/David%20Bowie?UserId=user-1",
            "/Artists/David%20Bowie?UserId=user-1",
            "/emby/Artists/David%20Bowie?userId=user-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("artist request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_item_prefix_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/Prefixes?UserId=user-1&IncludeItemTypes=Movie,MusicAlbum&Limit=50",
            "/Items/Prefixes?UserId=user-1&IncludeItemTypes=Movie,MusicAlbum&Limit=50",
            "/emby/Items/Prefixes?userId=user-1&parentId=library-1&recursive=true&startIndex=1&limit=50&includeItemTypes=Movie,MusicAlbum&searchTerm=mov&nameStartsWith=M&nameStartsWithOrGreater=L&nameLessThan=Z",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("item prefixes request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_person_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Persons?UserId=user-1&PersonTypes=Actor,Director&Limit=20",
            "/Persons?UserId=user-1&PersonTypes=Actor,Director&Limit=20",
            "/emby/Persons?userId=user-1&parentId=library-1&personTypes=Actor,Director&startIndex=1&limit=20&searchTerm=tom&sortOrder=Descending&nameStartsWith=T&nameStartsWithOrGreater=M&fields=PrimaryImageAspectRatio&enableImages=true",
            "/emby/Persons/Tom%20Hanks?UserId=user-1",
            "/Persons/Tom%20Hanks?UserId=user-1",
            "/emby/Persons/Tom%20Hanks?userId=user-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("person request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_studio_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Studios?UserId=user-1&Limit=20",
            "/Studios?UserId=user-1&Limit=20",
            "/emby/Studios?userId=user-1&parentId=library-1&startIndex=1&limit=20&searchTerm=studio&sortOrder=Descending&nameStartsWith=S&nameStartsWithOrGreater=M&fields=PrimaryImageAspectRatio&enableImages=true",
            "/emby/Studios/Studio%20A?UserId=user-1",
            "/Studios/Studio%20A?UserId=user-1",
            "/emby/Studios/Studio%20A?userId=user-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("studio request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_classification_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Tags?UserId=user-1&Limit=20",
            "/Tags?UserId=user-1&Limit=20",
            "/emby/Tags?userId=user-1&parentId=library-1&startIndex=1&limit=20&searchTerm=hdr&sortOrder=Descending&nameStartsWith=H&nameStartsWithOrGreater=G&fields=PrimaryImageAspectRatio&enableImages=true",
            "/emby/OfficialRatings?UserId=user-1&Limit=20",
            "/OfficialRatings?UserId=user-1&Limit=20",
            "/emby/OfficialRatings?userId=user-1&limit=20",
            "/emby/Years?UserId=user-1&ParentId=library-1&Limit=20",
            "/Years?UserId=user-1&ParentId=library-1&Limit=20",
            "/emby/Years?userId=user-1&parentId=library-1&limit=20",
            "/emby/Containers?UserId=user-1&Limit=20",
            "/Containers?UserId=user-1&Limit=20",
            "/emby/Containers?userId=user-1&limit=20",
            "/emby/AudioCodecs?UserId=user-1&Limit=20",
            "/AudioCodecs?UserId=user-1&Limit=20",
            "/emby/AudioCodecs?userId=user-1&limit=20",
            "/emby/VideoCodecs?UserId=user-1&Limit=20",
            "/VideoCodecs?UserId=user-1&Limit=20",
            "/emby/VideoCodecs?userId=user-1&limit=20",
            "/emby/SubtitleCodecs?UserId=user-1&Limit=20",
            "/SubtitleCodecs?UserId=user-1&Limit=20",
            "/emby/SubtitleCodecs?userId=user-1&limit=20",
            "/emby/StreamLanguages?UserId=user-1&Limit=20",
            "/StreamLanguages?UserId=user-1&Limit=20",
            "/emby/StreamLanguages?userId=user-1&limit=20",
            "/emby/Items/Filters?UserId=user-1&ParentId=library-1&IncludeItemTypes=Movie,Series&MediaTypes=Video",
            "/Items/Filters?UserId=user-1&ParentId=library-1&IncludeItemTypes=Movie,Series&MediaTypes=Video",
            "/emby/Items/Filters?userId=user-1&parentId=library-1&includeItemTypes=Movie,Series&mediaTypes=Video",
            "/emby/Items/Filters2?UserId=user-1&ParentId=library-1&IncludeItemTypes=Movie,Series&MediaTypes=Video",
            "/Items/Filters2?UserId=user-1&ParentId=library-1&IncludeItemTypes=Movie,Series&MediaTypes=Video",
            "/emby/Items/Filters2?userId=user-1&parentId=library-1&includeItemTypes=Movie,Series&mediaTypes=Video",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("classification request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_album_and_artist_similar_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Albums/album-1/Similar?UserId=user-1&Limit=12",
            "/Albums/album-1/Similar?UserId=user-1&Limit=12",
            "/emby/Artists/artist-1/Similar?UserId=user-1&Limit=12",
            "/Artists/artist-1/Similar?UserId=user-1&Limit=12",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("album or artist similar request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_artist_music_collection_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Artists/artist-1/Songs?UserId=user-1&Limit=20",
            "/emby/Artists/artist-1/Songs?userId=user-1&parentId=library-1&startIndex=0&limit=20&sortBy=SortName&sortOrder=Ascending&fields=MediaSources&searchTerm=blue&genreIds=1&artistIds=2&albumIds=3&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary,Backdrop",
            "/Artists/artist-1/Songs?UserId=user-1&Limit=20",
            "/emby/Artists/artist-1/Albums?UserId=user-1&Limit=20",
            "/emby/Artists/artist-1/Albums?userId=user-1&limit=20&enableImages=true&imageTypeLimit=2",
            "/Artists/artist-1/Albums?UserId=user-1&Limit=20",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("artist music collection request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_instant_mix_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/InstantMix?UserId=user-1&Limit=20",
            "/emby/Items/item-1/InstantMix?userId=user-1&startIndex=1&limit=20&fields=MediaSources&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary",
            "/Items/item-1/InstantMix?UserId=user-1&Limit=20",
            "/emby/Songs/song-1/InstantMix?UserId=user-1&Limit=20",
            "/emby/Songs/song-1/InstantMix?userId=user-1&startIndex=1&limit=20&fields=MediaSources&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary",
            "/Songs/song-1/InstantMix?UserId=user-1&Limit=20",
            "/emby/Albums/album-1/InstantMix?UserId=user-1&Limit=20",
            "/emby/Albums/album-1/InstantMix?userId=user-1&startIndex=1&limit=20&fields=MediaSources&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary",
            "/Albums/album-1/InstantMix?UserId=user-1&Limit=20",
            "/emby/Artists/InstantMix?UserId=user-1&Limit=20",
            "/emby/Artists/InstantMix?id=artist-1&userId=user-1&startIndex=1&limit=20&fields=MediaSources&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary",
            "/Artists/InstantMix?UserId=user-1&Limit=20",
            "/emby/MusicGenres/InstantMix?UserId=user-1&Limit=20",
            "/emby/MusicGenres/InstantMix?id=42&userId=user-1&startIndex=1&limit=20&fields=MediaSources&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary",
            "/MusicGenres/InstantMix?UserId=user-1&Limit=20",
            "/emby/MusicGenres/Rock/InstantMix?UserId=user-1&Limit=20",
            "/MusicGenres/Rock/InstantMix?UserId=user-1&Limit=20",
            "/emby/Playlists/playlist-1/InstantMix?UserId=user-1&Limit=20",
            "/Playlists/playlist-1/InstantMix?UserId=user-1&Limit=20",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("instant mix request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_playlist_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Playlists?UserId=user-1&Limit=20",
            "/Playlists?UserId=user-1&Limit=20",
            "/emby/Playlists?userId=user-1&parentId=library-1&startIndex=1&limit=20&searchTerm=mix&sortOrder=Descending&fields=PrimaryImageAspectRatio&enableImages=true",
            "/emby/Playlists/playlist-1/Items?UserId=user-1&Limit=50",
            "/Playlists/playlist-1/Items?UserId=user-1&Limit=50",
            "/emby/Playlists/playlist-1/Items?userId=user-1&startIndex=1&limit=50&fields=MediaSources&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary,Backdrop",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("playlist request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_playlist_write_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri) in [
            (
                Method::POST,
                "/emby/Playlists?Name=RoadTrip&Ids=item-1,item-2&MediaType=Audio",
            ),
            (
                Method::POST,
                "/emby/Playlists?name=RoadTrip&ids=item-1,item-2&mediaType=Audio",
            ),
            (
                Method::POST,
                "/Playlists?Name=RoadTrip&Ids=item-1,item-2&MediaType=Audio",
            ),
            (
                Method::GET,
                "/emby/Playlists/playlist-1/AddToPlaylistInfo?UserId=user-1&Ids=item-1,item-2",
            ),
            (
                Method::GET,
                "/emby/Playlists/playlist-1/AddToPlaylistInfo?userId=user-1&ids=item-1,item-2",
            ),
            (
                Method::GET,
                "/Playlists/playlist-1/AddToPlaylistInfo?UserId=user-1&Ids=item-1,item-2",
            ),
            (
                Method::POST,
                "/emby/Playlists/playlist-1/Items?UserId=user-1&Ids=item-1,item-2",
            ),
            (
                Method::POST,
                "/emby/Playlists/playlist-1/Items?userId=user-1&ids=item-1,item-2",
            ),
            (
                Method::POST,
                "/Playlists/playlist-1/Items?UserId=user-1&Ids=item-1,item-2",
            ),
            (
                Method::DELETE,
                "/emby/Playlists/playlist-1/Items?EntryIds=entry-1,entry-2",
            ),
            (
                Method::DELETE,
                "/emby/Playlists/playlist-1/Items?entryIds=entry-1,entry-2",
            ),
            (
                Method::DELETE,
                "/Playlists/playlist-1/Items?EntryIds=entry-1,entry-2",
            ),
            (
                Method::POST,
                "/emby/Playlists/playlist-1/Items/Delete?EntryIds=entry-1,entry-2",
            ),
            (
                Method::POST,
                "/emby/Playlists/playlist-1/Items/Delete?entryIds=entry-1,entry-2",
            ),
            (
                Method::POST,
                "/Playlists/playlist-1/Items/Delete?EntryIds=entry-1,entry-2",
            ),
            (
                Method::POST,
                "/emby/Playlists/playlist-1/Items/entry-1/Move/3",
            ),
            (Method::POST, "/Playlists/playlist-1/Items/entry-1/Move/3"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("playlist write request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_collection_write_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri) in [
            (
                Method::POST,
                "/emby/Collections?Name=Favorites&Ids=item-1,item-2&IsLocked=true",
            ),
            (
                Method::POST,
                "/emby/Collections?name=Favorites&ids=item-1,item-2&isLocked=true&parentId=parent-1",
            ),
            (
                Method::POST,
                "/Collections?Name=Favorites&Ids=item-1,item-2&IsLocked=true",
            ),
            (
                Method::POST,
                "/emby/Collections/collection-1/Items?Ids=item-1,item-2",
            ),
            (
                Method::POST,
                "/emby/Collections/collection-1/Items?ids=item-1,item-2",
            ),
            (
                Method::POST,
                "/Collections/collection-1/Items?Ids=item-1,item-2",
            ),
            (
                Method::DELETE,
                "/emby/Collections/collection-1/Items?Ids=item-1,item-2",
            ),
            (
                Method::DELETE,
                "/emby/Collections/collection-1/Items?ids=item-1,item-2",
            ),
            (
                Method::DELETE,
                "/Collections/collection-1/Items?Ids=item-1,item-2",
            ),
            (
                Method::POST,
                "/emby/Collections/collection-1/Items/Delete?Ids=item-1,item-2",
            ),
            (
                Method::POST,
                "/emby/Collections/collection-1/Items/Delete?ids=item-1,item-2",
            ),
            (
                Method::POST,
                "/Collections/collection-1/Items/Delete?Ids=item-1,item-2",
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("collection write request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_item_by_id_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Users/user-1/Items/item-1",
            "/Users/user-1/Items/item-1",
            "/emby/Items/item-1?UserId=user-1&Fields=Chapters",
            "/emby/Items/item-1?userId=user-1&fields=Chapters",
            "/Items/item-1?UserId=user-1&Fields=Chapters",
            "/emby/Items/item-1/DeleteInfo?UserId=user-1",
            "/Items/item-1/DeleteInfo?UserId=user-1",
            "/emby/Items/item-1/CriticReviews?UserId=user-1&StartIndex=4&Limit=8",
            "/emby/Items/item-1/CriticReviews?userId=user-1&startIndex=4&limit=8",
            "/Items/item-1/CriticReviews?UserId=user-1&StartIndex=4&Limit=8",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("item request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_item_image_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri) in [
            (Method::GET, "/emby/Items/item-1/Images"),
            (Method::GET, "/Items/item-1/Images"),
            (Method::GET, "/emby/Items/item-1/Images/Primary"),
            (Method::GET, "/Items/item-1/Images/Primary"),
            (Method::HEAD, "/emby/Items/item-1/Images/Primary"),
            (Method::HEAD, "/Items/item-1/Images/Primary"),
            (Method::GET, "/emby/Items/item-1/Images/Backdrop/0"),
            (Method::GET, "/Items/item-1/Images/Backdrop/0"),
            (Method::HEAD, "/emby/Items/item-1/Images/Backdrop/0"),
            (Method::HEAD, "/Items/item-1/Images/Backdrop/0"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("item image request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_item_image_mutation_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri, body) in [
            (
                Method::POST,
                "/emby/Items/item-1/Images/Primary?Index=0",
                Some("base64-image"),
            ),
            (
                Method::POST,
                "/Items/item-1/Images/Primary?Index=0",
                Some("base64-image"),
            ),
            (
                Method::POST,
                "/emby/Items/item-1/Images/Backdrop/1",
                Some("base64-image"),
            ),
            (
                Method::POST,
                "/Items/item-1/Images/Backdrop/1",
                Some("base64-image"),
            ),
            (
                Method::DELETE,
                "/emby/Items/item-1/Images/Primary?Index=0",
                None,
            ),
            (Method::DELETE, "/Items/item-1/Images/Primary?Index=0", None),
            (Method::DELETE, "/emby/Items/item-1/Images/Backdrop/1", None),
            (Method::DELETE, "/Items/item-1/Images/Backdrop/1", None),
            (
                Method::POST,
                "/emby/Items/item-1/Images/Primary/Delete?Index=0",
                None,
            ),
            (
                Method::POST,
                "/Items/item-1/Images/Primary/Delete?Index=0",
                None,
            ),
            (
                Method::POST,
                "/emby/Items/item-1/Images/Backdrop/1/Delete",
                None,
            ),
            (Method::POST, "/Items/item-1/Images/Backdrop/1/Delete", None),
            (
                Method::POST,
                "/emby/Items/item-1/Images/Backdrop/1/Index?NewIndex=0",
                None,
            ),
            (
                Method::POST,
                "/Items/item-1/Images/Backdrop/1/Index?NewIndex=0",
                None,
            ),
            (
                Method::POST,
                "/emby/Items/item-1/Images/Backdrop/1/Url?Url=https%3A%2F%2Fimage.example.test%2Fbackdrop.jpg",
                None,
            ),
            (
                Method::POST,
                "/Items/item-1/Images/Backdrop/1/Url?Url=https%3A%2F%2Fimage.example.test%2Fbackdrop.jpg",
                None,
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(body.map_or_else(Body::empty, Body::from))
                        .expect("request should build"),
                )
                .await
                .expect("item image mutation request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        }
    }

    #[tokio::test]
    async fn emby_remote_image_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri, body) in [
            (
                Method::GET,
                "/emby/Items/item-1/RemoteImages?Type=Primary&StartIndex=0&Limit=20&ProviderName=TheMovieDb&IncludeAllLanguages=true&EnableSeriesImages=false",
                None,
            ),
            (
                Method::GET,
                "/Items/item-1/RemoteImages?Type=Primary&StartIndex=0&Limit=20&ProviderName=TheMovieDb&IncludeAllLanguages=true&EnableSeriesImages=false",
                None,
            ),
            (
                Method::GET,
                "/emby/Items/item-1/RemoteImages/Providers",
                None,
            ),
            (Method::GET, "/Items/item-1/RemoteImages/Providers", None),
            (
                Method::POST,
                "/emby/Items/item-1/RemoteImages/Download?Type=Primary&ProviderName=TheMovieDb&ImageUrl=https%3A%2F%2Fimage.example.test%2Fposter.jpg",
                Some(r#"{"ImageIndex":0}"#),
            ),
            (
                Method::POST,
                "/Items/item-1/RemoteImages/Download?Type=Primary&ProviderName=TheMovieDb&ImageUrl=https%3A%2F%2Fimage.example.test%2Fposter.jpg",
                Some(r#"{"ImageIndex":0}"#),
            ),
            (
                Method::GET,
                "/emby/Images/Remote?ImageUrl=https%3A%2F%2Fimage.example.test%2Fposter.jpg",
                None,
            ),
            (
                Method::GET,
                "/Images/Remote?ImageUrl=https%3A%2F%2Fimage.example.test%2Fposter.jpg",
                None,
            ),
        ] {
            let mut builder = Request::builder()
                .method(method)
                .uri(uri)
                .header("x-emby-token", "test-token");
            if body.is_some() {
                builder = builder.header("content-type", "application/json");
            }

            let response = app
                .clone()
                .oneshot(
                    builder
                        .body(body.map_or_else(Body::empty, Body::from))
                        .expect("request should build"),
                )
                .await
                .expect("remote image request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        }
    }

    #[tokio::test]
    async fn emby_item_lookup_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri, body) in [
            (Method::GET, "/emby/Items/item-1/ExternalIdInfos", None),
            (Method::GET, "/Items/item-1/ExternalIdInfos", None),
            (Method::GET, "/emby/Items/item-1/MetadataEditor", None),
            (Method::GET, "/Items/item-1/MetadataEditor", None),
            (
                Method::GET,
                "/emby/Items/RemoteSearch/Image?ImageUrl=https%3A%2F%2Fimage.example.test%2Fposter.jpg&ProviderName=TheMovieDb",
                None,
            ),
            (
                Method::GET,
                "/Items/RemoteSearch/Image?ImageUrl=https%3A%2F%2Fimage.example.test%2Fposter.jpg&ProviderName=TheMovieDb",
                None,
            ),
            (
                Method::POST,
                "/emby/Items/Metadata/Reset?ItemIds=item-1,item-2",
                None,
            ),
            (
                Method::POST,
                "/Items/Metadata/Reset?ItemIds=item-1,item-2",
                None,
            ),
            (
                Method::POST,
                "/emby/Items/RemoteSearch/Apply/item-1?ReplaceAllImages=true",
                Some(r#"{"Name":"Movie Result","ProviderIds":{"Tmdb":"42"}}"#),
            ),
            (
                Method::POST,
                "/emby/Items/RemoteSearch/Apply/item-1?replaceAllImages=true",
                Some(
                    r#"{"name":"Movie Result","providerIds":{"Tmdb":"42"},"searchProviderName":"TheMovieDb"}"#,
                ),
            ),
            (
                Method::POST,
                "/Items/RemoteSearch/Apply/item-1?ReplaceAllImages=true",
                Some(r#"{"Name":"Movie Result","ProviderIds":{"Tmdb":"42"}}"#),
            ),
            (
                Method::POST,
                "/emby/Items/RemoteSearch/Movie",
                Some(
                    r#"{"SearchInfo":{"Name":"A Movie","Year":2024},"ItemId":42,"SearchProviderName":"TheMovieDb","Providers":["TheMovieDb"],"IncludeDisabledProviders":false}"#,
                ),
            ),
            (
                Method::POST,
                "/emby/Items/RemoteSearch/Movie",
                Some(
                    r#"{"searchInfo":{"name":"A Movie","metadataLanguage":"en","metadataCountryCode":"US","providerIds":{"Tmdb":"42"},"year":2024,"indexNumber":1,"parentIndexNumber":2,"premiereDate":"2024-01-01","isAutomated":true,"enableAdultMetadata":false},"itemId":42,"searchProviderName":"TheMovieDb","providers":["TheMovieDb"],"includeDisabledProviders":false}"#,
                ),
            ),
            (
                Method::POST,
                "/Items/RemoteSearch/Movie",
                Some(
                    r#"{"SearchInfo":{"Name":"A Movie","Year":2024},"ItemId":42,"SearchProviderName":"TheMovieDb","Providers":["TheMovieDb"],"IncludeDisabledProviders":false}"#,
                ),
            ),
            (
                Method::POST,
                "/emby/Items/RemoteSearch/Series",
                Some(r#"{"SearchInfo":{"Name":"A Series","Year":2024}}"#),
            ),
            (
                Method::POST,
                "/Items/RemoteSearch/Series",
                Some(r#"{"SearchInfo":{"Name":"A Series","Year":2024}}"#),
            ),
            (
                Method::POST,
                "/emby/Items/RemoteSearch/MusicArtist",
                Some(r#"{"SearchInfo":{"Name":"An Artist"}}"#),
            ),
            (
                Method::POST,
                "/Items/RemoteSearch/MusicArtist",
                Some(r#"{"SearchInfo":{"Name":"An Artist"}}"#),
            ),
            (
                Method::POST,
                "/emby/Items/RemoteSearch/MusicAlbum",
                Some(r#"{"SearchInfo":{"Name":"An Album"}}"#),
            ),
            (
                Method::POST,
                "/Items/RemoteSearch/MusicAlbum",
                Some(r#"{"SearchInfo":{"Name":"An Album"}}"#),
            ),
        ] {
            let mut builder = Request::builder()
                .method(method)
                .uri(uri)
                .header("x-emby-token", "test-token");
            if body.is_some() {
                builder = builder.header("content-type", "application/json");
            }

            let response = app
                .clone()
                .oneshot(
                    builder
                        .body(body.map_or_else(Body::empty, Body::from))
                        .expect("request should build"),
                )
                .await
                .expect("item lookup request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_remote_subtitle_search_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/RemoteSearch/Subtitles/eng?MediaSourceId=source-1&UserId=user-1",
            "/emby/Items/item-1/RemoteSearch/Subtitles/eng?mediaSourceId=source-1&userId=user-1&isPerfectMatch=true&isForced=false&isHearingImpaired=true",
            "/Items/item-1/RemoteSearch/Subtitles/eng?MediaSourceId=source-1&UserId=user-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("remote subtitle search request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_subtitle_management_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri) in [
            (
                Method::GET,
                "/emby/Providers/Subtitles/Subtitles/sub-1?api_key=test-token",
            ),
            (
                Method::GET,
                "/Providers/Subtitles/Subtitles/sub-1?api_key=test-token",
            ),
            (
                Method::POST,
                "/emby/Items/item-1/RemoteSearch/Subtitles/sub-1?MediaSourceId=42",
            ),
            (
                Method::POST,
                "/emby/Items/item-1/RemoteSearch/Subtitles/sub-1?mediaSourceId=42",
            ),
            (
                Method::POST,
                "/Items/item-1/RemoteSearch/Subtitles/sub-1?MediaSourceId=42",
            ),
            (
                Method::DELETE,
                "/emby/Items/item-1/Subtitles/3?MediaSourceId=42",
            ),
            (
                Method::DELETE,
                "/emby/Items/item-1/Subtitles/3?mediaSourceId=42",
            ),
            (Method::DELETE, "/Items/item-1/Subtitles/3?MediaSourceId=42"),
            (
                Method::POST,
                "/emby/Items/item-1/Subtitles/3/Delete?MediaSourceId=42",
            ),
            (
                Method::POST,
                "/Items/item-1/Subtitles/3/Delete?MediaSourceId=42",
            ),
            (
                Method::DELETE,
                "/emby/Videos/item-1/Subtitles/3?MediaSourceId=42",
            ),
            (
                Method::DELETE,
                "/Videos/item-1/Subtitles/3?MediaSourceId=42",
            ),
            (
                Method::POST,
                "/emby/Videos/item-1/Subtitles/3/Delete?MediaSourceId=42",
            ),
            (
                Method::POST,
                "/emby/Videos/item-1/Subtitles/3/Delete?mediaSourceId=42",
            ),
            (
                Method::POST,
                "/Videos/item-1/Subtitles/3/Delete?MediaSourceId=42",
            ),
            (
                Method::GET,
                "/emby/Videos/item-1/42/Attachments/4/Stream?api_key=test-token",
            ),
            (
                Method::GET,
                "/Videos/item-1/42/Attachments/4/Stream?api_key=test-token",
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("subtitle management request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_subtitle_stream_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/42/Subtitles/3/Stream.srt?StartPositionTicks=0",
            "/emby/Items/item-1/42/Subtitles/3/Stream.srt?startPositionTicks=0&endPositionTicks=100&copyTimestamps=true&userId=user-1",
            "/Items/item-1/42/Subtitles/3/Stream.srt?StartPositionTicks=0",
            "/emby/Items/item-1/42/Subtitles/3/0/Stream.srt",
            "/Items/item-1/42/Subtitles/3/0/Stream.srt",
            "/emby/Videos/item-1/42/Subtitles/3/Stream.srt?StartPositionTicks=0",
            "/Videos/item-1/42/Subtitles/3/Stream.srt?StartPositionTicks=0",
            "/emby/Videos/item-1/42/Subtitles/3/0/Stream.srt",
            "/Videos/item-1/42/Subtitles/3/0/Stream.srt",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("subtitle stream request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_hls_subtitle_playlist_aliases_do_not_require_transcode_session() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Videos/item-1/subtitles.m3u8?SubtitleSegmentLength=4&ManifestSubtitles=vtt",
            "/emby/Videos/item-1/subtitles.m3u8?subtitleSegmentLength=4&manifestSubtitles=vtt&userId=user-1",
            "/Videos/item-1/subtitles.m3u8?SubtitleSegmentLength=4&ManifestSubtitles=vtt",
            "/emby/Videos/item-1/live_subtitles.m3u8?SubtitleSegmentLength=4&ManifestSubtitles=vtt",
            "/Videos/item-1/live_subtitles.m3u8?SubtitleSegmentLength=4&ManifestSubtitles=vtt",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("hls subtitle playlist request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::BAD_REQUEST);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_item_ancestors_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/Ancestors?UserId=user-1",
            "/Items/item-1/Ancestors?UserId=user-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("item ancestors request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_similar_items_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/Similar?UserId=user-1&Limit=12",
            "/emby/Items/item-1/Similar?userId=user-1&startIndex=1&limit=12&includeItemTypes=Movie&sortBy=SortName&sortOrder=Ascending&fields=MediaSources",
            "/Items/item-1/Similar?UserId=user-1&Limit=12",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("similar items request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_movie_recommendation_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Movies/Recommendations?UserId=user-1&CategoryLimit=1&ItemLimit=12&EnableImages=true",
            "/emby/Movies/Recommendations?userId=user-1&parentId=movies&categoryLimit=1&itemLimit=12&enableImages=true&enableUserData=false&imageTypeLimit=2&enableImageTypes=Primary",
            "/Movies/Recommendations?UserId=user-1&CategoryLimit=1&ItemLimit=12&EnableImages=true",
            "/emby/Movies/movie-1/Similar?UserId=user-1&Limit=12",
            "/emby/Movies/movie-1/Similar?userId=user-1&startIndex=1&limit=12&includeItemTypes=Movie&sortBy=SortName&sortOrder=Ascending&fields=MediaSources",
            "/Movies/movie-1/Similar?UserId=user-1&Limit=12",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("movie recommendation request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_theme_media_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/ThemeMedia?UserId=user-1",
            "/Items/item-1/ThemeMedia?UserId=user-1",
            "/emby/Items/item-1/ThemeMedia?userId=user-1&inheritFromParent=true&startIndex=0&limit=10&fields=MediaSources",
            "/emby/Items/item-1/ThemeSongs?UserId=user-1",
            "/Items/item-1/ThemeSongs?UserId=user-1",
            "/emby/Items/item-1/ThemeSongs?userId=user-1&inheritFromParent=true&startIndex=0&limit=10&fields=MediaSources",
            "/emby/Items/item-1/ThemeVideos?UserId=user-1",
            "/Items/item-1/ThemeVideos?UserId=user-1",
            "/emby/Items/item-1/ThemeVideos?userId=user-1&inheritFromParent=true&startIndex=0&limit=10&fields=MediaSources",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("theme media request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_special_features_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/SpecialFeatures?UserId=user-1&StartIndex=0&Limit=10",
            "/emby/Items/item-1/SpecialFeatures?userId=user-1&startIndex=0&limit=10&fields=MediaSources&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary&enableUserData=true",
            "/Items/item-1/SpecialFeatures?UserId=user-1&StartIndex=0&Limit=10",
            "/emby/Users/user-1/Items/item-1/SpecialFeatures?StartIndex=0&Limit=10",
            "/emby/Users/user-1/Items/item-1/SpecialFeatures?startIndex=0&limit=10&fields=MediaSources&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary&enableUserData=true",
            "/Users/user-1/Items/item-1/SpecialFeatures?StartIndex=0&Limit=10",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("special features request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_playback_extra_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/Intros?UserId=user-1&Fields=MediaSources",
            "/emby/Items/item-1/Intros?userId=user-1&startIndex=1&limit=4&fields=MediaSources&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary&enableUserData=false",
            "/Items/item-1/Intros?UserId=user-1&Fields=MediaSources",
            "/emby/Users/user-1/Items/item-1/Intros?Fields=MediaSources",
            "/emby/Users/user-1/Items/item-1/Intros?startIndex=1&limit=4&fields=MediaSources&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary&enableUserData=false",
            "/Users/user-1/Items/item-1/Intros?Fields=MediaSources",
            "/emby/Items/item-1/LocalTrailers?UserId=user-1&Fields=MediaSources",
            "/emby/Items/item-1/LocalTrailers?userId=user-1&startIndex=1&limit=4&fields=MediaSources&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary&enableUserData=false",
            "/Items/item-1/LocalTrailers?UserId=user-1&Fields=MediaSources",
            "/emby/Users/user-1/Items/item-1/LocalTrailers?Fields=MediaSources",
            "/emby/Users/user-1/Items/item-1/LocalTrailers?startIndex=1&limit=4&fields=MediaSources&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary&enableUserData=false",
            "/Users/user-1/Items/item-1/LocalTrailers?Fields=MediaSources",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("playback extra request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_lyrics_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Audio/item-1/Lyrics?UserId=user-1",
            "/emby/Audio/item-1/Lyrics?userId=user-1",
            "/Audio/item-1/Lyrics?UserId=user-1",
            "/emby/Items/item-1/Lyrics?UserId=user-1",
            "/emby/Items/item-1/Lyrics?userId=user-1",
            "/Items/item-1/Lyrics?UserId=user-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("lyrics request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_remote_lyrics_search_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Audio/item-1/RemoteSearch/Lyrics?UserId=user-1&MediaSourceId=42",
            "/emby/Audio/item-1/RemoteSearch/Lyrics?userId=user-1&mediaSourceId=42&providerName=LrcLib&searchTerm=Signal",
            "/Audio/item-1/RemoteSearch/Lyrics?UserId=user-1&MediaSourceId=42",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("remote lyrics search request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_playback_info_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/PlaybackInfo",
            "/Items/item-1/PlaybackInfo",
            "/emby/Users/user-1/Items/item-1/PlaybackInfo",
            "/Users/user-1/Items/item-1/PlaybackInfo",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("playback info request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_post_playback_info_accepts_query_only_payload() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/PlaybackInfo?UserId=user-1&MediaSourceId=42&MaxStreamingBitrate=8000000",
            "/Items/item-1/PlaybackInfo?UserId=user-1&MediaSourceId=42&MaxStreamingBitrate=8000000",
            "/emby/Users/user-1/Items/item-1/PlaybackInfo?UserId=user-1&MediaSourceId=42&MaxStreamingBitrate=8000000",
            "/Users/user-1/Items/item-1/PlaybackInfo?UserId=user-1&MediaSourceId=42&MaxStreamingBitrate=8000000",
            "/emby/Items/item-1/PlaybackInfo?userId=user-1&mediaSourceId=42&maxStreamingBitrate=8000000&startTimeTicks=100",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("playback info request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_playback_bitrate_test_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Playback/BitrateTest?Size=1024",
            "/Playback/BitrateTest?Size=1024",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("bitrate test request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_item_download_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/Download?MediaSourceId=42&api_key=test-token",
            "/Items/item-1/Download?MediaSourceId=42&api_key=test-token",
            "/emby/Items/item-1/Download?mediaSourceId=42&api_key=test-token",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("download request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_item_file_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/File?MediaSourceId=42&api_key=test-token",
            "/Items/item-1/File?MediaSourceId=42&api_key=test-token",
            "/Items/item-1/File?mediaSourceId=42&api_key=test-token",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("item file request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_item_refresh_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/Refresh?Recursive=true&MetadataRefreshMode=FullRefresh&ImageRefreshMode=Default&ReplaceAllMetadata=true&ReplaceAllImages=false",
            "/Items/item-1/Refresh?Recursive=true&MetadataRefreshMode=FullRefresh&ImageRefreshMode=Default&ReplaceAllMetadata=true&ReplaceAllImages=false",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(Body::from(r#"{"ReplaceThumbnailImages":true}"#))
                        .expect("request should build"),
                )
                .await
                .expect("item refresh request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_hls_transcoding_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Audio/ActiveEncodings?PlaySessionId=play-1&DeviceId=device-1",
            "/emby/Audio/ActiveEncodings?playSessionId=play-1&deviceId=device-1",
            "/Audio/ActiveEncodings?PlaySessionId=play-1&DeviceId=device-1",
            "/emby/Videos/ActiveEncodings?PlaySessionId=play-1&DeviceId=device-1",
            "/emby/Videos/ActiveEncodings?playSessionId=play-1&deviceId=device-1",
            "/Videos/ActiveEncodings?PlaySessionId=play-1&DeviceId=device-1",
            "/emby/Videos/item-1/master.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/emby/Videos/item-1/master.m3u8?transcodeSessionId=session-1&mediaSourceId=42&api_key=test-token",
            "/emby/Videos/item-1/main.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/emby/Videos/item-1/live.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/emby/videos/item-1/master.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/videos/item-1/master.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/Videos/item-1/master.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/emby/Audio/item-1/master.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/emby/Audio/item-1/master.m3u8?transcodeSessionId=session-1&mediaSourceId=42&api_key=test-token",
            "/emby/Audio/item-1/main.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/emby/Audio/item-1/live.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/Audio/item-1/master.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/emby/Videos/item-1/hls1/master/0.ts?TranscodeSessionId=session-1&api_key=test-token",
            "/emby/Videos/item-1/hls1/master/0.ts?transcodeSessionId=session-1&mediaSourceId=42&api_key=test-token",
            "/Videos/item-1/hls1/master/0.ts?TranscodeSessionId=session-1&api_key=test-token",
            "/emby/Audio/item-1/hls1/master/0.ts?TranscodeSessionId=session-1&api_key=test-token",
            "/emby/Audio/item-1/hls1/master/0.ts?transcodeSessionId=session-1&mediaSourceId=42&api_key=test-token",
            "/Audio/item-1/hls1/master/0.ts?TranscodeSessionId=session-1&api_key=test-token",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(if uri.contains("ActiveEncodings") {
                            Method::DELETE
                        } else {
                            Method::GET
                        })
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("hls transcoding request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_lowercase_audio_playback_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/audio/ActiveEncodings?PlaySessionId=play-1&DeviceId=device-1",
            "/emby/audio/ActiveEncodings?playSessionId=play-1&deviceId=device-1",
            "/audio/ActiveEncodings?PlaySessionId=play-1&DeviceId=device-1",
            "/emby/audio/item-1/master.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/emby/audio/item-1/master.m3u8?transcodeSessionId=session-1&mediaSourceId=42&api_key=test-token",
            "/audio/item-1/master.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/emby/audio/item-1/main.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/audio/item-1/live.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/emby/audio/item-1/hls1/master/0.ts?TranscodeSessionId=session-1&api_key=test-token",
            "/audio/item-1/hls1/master/0.ts?TranscodeSessionId=session-1&api_key=test-token",
            "/emby/audio/item-1/universal?UserId=user-1&api_key=test-token",
            "/audio/item-1/universal?UserId=user-1&api_key=test-token",
            "/emby/audio/item-1/stream.mp3?MediaSourceId=42&api_key=test-token",
            "/audio/item-1/stream.mp3?MediaSourceId=42&api_key=test-token",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(if uri.contains("ActiveEncodings") {
                            Method::DELETE
                        } else {
                            Method::GET
                        })
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("audio playback alias request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        }
    }

    #[tokio::test]
    async fn emby_video_stream_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Videos/item-1/AdditionalParts?UserId=user-1&Fields=MediaSources",
            "/emby/Videos/item-1/AdditionalParts?userId=user-1&fields=MediaSources&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary&enableUserData=false",
            "/Videos/item-1/AdditionalParts?UserId=user-1&Fields=MediaSources",
            "/emby/Videos/item-1/stream?MediaSourceId=42&api_key=test-token",
            "/emby/Videos/item-1/stream?mediaSourceId=42&api_key=test-token",
            "/Videos/item-1/stream?MediaSourceId=42&api_key=test-token",
            "/emby/Videos/item-1/stream.mkv?MediaSourceId=42&api_key=test-token",
            "/emby/Videos/item-1/stream.mkv?mediaSourceId=42&api_key=test-token",
            "/Videos/item-1/stream.mkv?MediaSourceId=42&api_key=test-token",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("video stream request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_video_stream_head_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Videos/item-1/stream?MediaSourceId=42&api_key=test-token",
            "/Videos/item-1/stream?MediaSourceId=42&api_key=test-token",
            "/emby/Videos/item-1/stream.mkv?MediaSourceId=42&api_key=test-token",
            "/Videos/item-1/stream.mkv?MediaSourceId=42&api_key=test-token",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::HEAD)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("video stream head request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        }
    }

    #[tokio::test]
    async fn emby_video_bif_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Videos/item-1/index.bif?Width=320",
            "/Videos/item-1/index.bif?Width=320",
            "/emby/Items/item-1/ThumbnailSet?Width=320",
            "/Items/item-1/ThumbnailSet?Width=320",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("bif request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        }
    }

    #[tokio::test]
    async fn emby_video_bif_validates_width_before_stream_boundary() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Videos/item-1/index.bif?Width=0",
            "/Videos/item-1/index.bif?Width=0",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("bif request should succeed");

            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_video_stream_file_name_aliases_use_stream_boundary() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Videos/item-1/movie.mkv?MediaSourceId=42&api_key=test-token",
            "/Videos/item-1/movie.mkv?MediaSourceId=42&api_key=test-token",
            "/emby/videos/item-1/movie.mkv?MediaSourceId=42&api_key=test-token",
            "/emby/Videos/item-1/movie.mkv?transcodeSessionId=session-1&mediaSourceId=42&api_key=test-token",
            "/videos/item-1/movie.mkv?MediaSourceId=42&api_key=test-token",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("video stream file-name request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::BAD_REQUEST);
        }
    }

    #[tokio::test]
    async fn emby_livestream_media_info_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (uri, body) in [
            (
                "/emby/LiveStreams/Open",
                Body::from(r#"{"UserId":"user-1"}"#),
            ),
            ("/LiveStreams/Open", Body::from(r#"{"UserId":"user-1"}"#)),
            (
                "/emby/LiveStreams/Open",
                Body::from(r#"{"openToken":"open-1","userId":"user-1","playSessionId":"play-1"}"#),
            ),
            (
                "/emby/LiveStreams/MediaInfo?LiveStreamId=live-1",
                Body::empty(),
            ),
            ("/LiveStreams/MediaInfo?liveStreamId=live-1", Body::empty()),
            ("/LiveStreams/MediaInfo?LiveStreamId=live-1", Body::empty()),
            ("/emby/LiveStreams/Close?LiveStreamId=live-1", Body::empty()),
            ("/emby/LiveStreams/Close?liveStreamId=live-1", Body::empty()),
            ("/LiveStreams/Close?LiveStreamId=live-1", Body::empty()),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(body)
                        .expect("request should build"),
                )
                .await
                .expect("live stream request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_audio_stream_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Audio/item-1/universal?UserId=user-1&DeviceId=device-1&MediaSourceId=42&MaxStreamingBitrate=140000000&Container=mp3,aac,flac&PlaySessionId=play-1&api_key=test-token",
            "/emby/Audio/item-1/universal?userId=user-1&deviceId=device-1&mediaSourceId=42&maxStreamingBitrate=140000000&audioBitRate=320000&container=mp3,aac,flac&playSessionId=play-1&transcodingProtocol=hls&transcodingContainer=ts&audioCodec=aac&startTimeTicks=100&api_key=test-token",
            "/Audio/item-1/universal?UserId=user-1&DeviceId=device-1&MediaSourceId=42&MaxStreamingBitrate=140000000&Container=mp3,aac,flac&PlaySessionId=play-1&api_key=test-token",
            "/emby/Audio/item-1/stream.mp3?MediaSourceId=42&api_key=test-token",
            "/emby/Audio/item-1/stream.mp3?mediaSourceId=42&api_key=test-token",
            "/Audio/item-1/stream.mp3?MediaSourceId=42&api_key=test-token",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("audio stream request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_playback_report_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Sessions/Playing",
            "/Sessions/Playing",
            "/emby/Sessions/Playing/Ping?PlaySessionId=play-1",
            "/Sessions/Playing/Ping?PlaySessionId=play-1",
            "/emby/Sessions/Playing/Ping?playSessionId=play-1",
            "/emby/Sessions/Playing/Progress",
            "/Sessions/Playing/Progress",
            "/emby/Sessions/Playing/Stopped",
            "/Sessions/Playing/Stopped",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(Body::from(r#"{"ItemId":"item-1","PositionTicks":42}"#))
                        .expect("request should build"),
                )
                .await
                .expect("playback report request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_playback_report_query_aliases_parse_before_repository_boundary() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Sessions/Playing?ItemId=item-1&UserId=user-1&PlaySessionId=play-1&MediaSourceId=42&PlayMethod=DirectStream&PositionTicks=42",
            "/Sessions/Playing/Progress?ItemId=item-1&UserId=user-1&PlaySessionId=play-1&MediaSourceId=42&IsPaused=true&PositionTicks=42",
            "/emby/Sessions/Playing/Stopped?ItemId=item-1&UserId=user-1&PlaySessionId=play-1&MediaSourceId=42&PositionTicks=42",
            "/Sessions/Playing/Progress?itemId=item-1&userId=user-1&playSessionId=play-1&mediaSourceId=42&playMethod=DirectStream&queueableMediaTypes=Audio%2CVideo&positionTicks=42&isPaused=true&playlistItemIds=playlist-item-1%2Cplaylist-item-2",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("query playback report request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_playback_report_accepts_lower_camel_body_fields() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Sessions/Playing",
            "/Sessions/Playing/Progress",
            "/emby/Sessions/Playing/Stopped",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(Body::from(
                            r#"{"item":{"id":"item-1"},"userId":"user-1","playSessionId":"play-1","mediaSourceId":"42","playMethod":"DirectStream","queueableMediaTypes":"Audio,Video","positionTicks":42,"isPaused":true,"playlistItemIds":"playlist-item-1,playlist-item-2"}"#,
                        ))
                        .expect("request should build"),
                )
                .await
                .expect("lower-camel playback report request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_user_playing_items_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri) in [
            (Method::POST, "/emby/Users/user-1/PlayingItems/item-1"),
            (Method::POST, "/Users/user-1/PlayingItems/item-1"),
            (
                Method::POST,
                "/emby/Users/user-1/PlayingItems/item-1/Progress",
            ),
            (Method::POST, "/Users/user-1/PlayingItems/item-1/Progress"),
            (Method::DELETE, "/emby/Users/user-1/PlayingItems/item-1"),
            (Method::DELETE, "/Users/user-1/PlayingItems/item-1"),
            (
                Method::POST,
                "/emby/Users/user-1/PlayingItems/item-1/Delete",
            ),
            (Method::POST, "/Users/user-1/PlayingItems/item-1/Delete"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .header("x-emby-token", "test-token")
                        .body(Body::from(r#"{"PositionTicks":42}"#))
                        .expect("request should build"),
                )
                .await
                .expect("user playing item request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_user_data_write_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        let cases = [
            (Method::POST, "/emby/Users/user-1/PlayedItems/item-1"),
            (Method::DELETE, "/Users/user-1/PlayedItems/item-1"),
            (Method::POST, "/emby/Users/user-1/PlayedItems/item-1/Delete"),
            (Method::POST, "/emby/Users/user-1/FavoriteItems/item-1"),
            (Method::DELETE, "/Users/user-1/FavoriteItems/item-1"),
            (
                Method::POST,
                "/emby/Users/user-1/FavoriteItems/item-1/Delete",
            ),
            (
                Method::POST,
                "/emby/Users/user-1/Items/item-1/Rating?Likes=true",
            ),
            (
                Method::POST,
                "/emby/Users/user-1/Items/item-1/Rating?likes=true",
            ),
            (Method::DELETE, "/Users/user-1/Items/item-1/Rating"),
            (
                Method::POST,
                "/emby/Users/user-1/Items/item-1/Rating/Delete",
            ),
            (
                Method::POST,
                "/emby/Users/user-1/Items/item-1/HideFromResume?Hide=true",
            ),
            (
                Method::POST,
                "/emby/Users/user-1/Items/item-1/HideFromResume?hide=true",
            ),
            (
                Method::POST,
                "/Users/user-1/Items/item-1/HideFromResume?Hide=false",
            ),
        ];

        for (method, uri) in cases {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("user data request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
            assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn emby_user_data_read_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Users/user-1/Items/item-1/UserData",
            "/Users/user-1/Items/item-1/UserData",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .header("x-emby-token", "test-token")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("user data request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_user_data_full_update_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Users/user-1/Items/item-1/UserData",
            "/Users/user-1/Items/item-1/UserData",
        ] {
            for body in [
                r#"{"PlaybackPositionTicks":120000,"PlayCount":3,"Played":false,"IsFavorite":true,"Rating":8.5}"#,
                r#"{"playbackPositionTicks":120000,"playCount":3,"played":false,"isFavorite":true,"rating":8.5,"lastPlayedDate":"2026-01-01T00:00:00Z"}"#,
            ] {
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method(Method::POST)
                            .uri(uri)
                            .header("content-type", "application/json")
                            .header("x-emby-token", "test-token")
                            .body(Body::from(body))
                            .expect("request should build"),
                    )
                    .await
                    .expect("user data request should succeed");

                assert_ne!(response.status(), StatusCode::NOT_FOUND);
                assert_ne!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
            }
        }
    }

    #[tokio::test]
    async fn plugin_host_kv_route_exists() {
        let app = build_router(AppState::for_tests(Config::default()));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/api/plugin/kv/state.cursor")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"value":{"cursor":"1"}}"#))
                    .expect("request should build"),
            )
            .await
            .expect("plugin host kv request should succeed");

        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn plugin_host_capabilities_route_exists() {
        let app = build_router(AppState::for_tests(Config::default()));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/plugin/capabilities")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("plugin host capabilities request should succeed");

        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn plugin_host_library_routes_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/api/plugin/libraries",
            "/api/plugin/libraries/library-1/items",
            "/api/plugin/items/item-1",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("plugin host library request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn plugin_host_notification_route_exists() {
        let app = build_router(AppState::for_tests(Config::default()));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/plugin/notifications")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"title":"Scan complete","message":"2 new items"}"#,
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("plugin host notification request should succeed");

        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn plugin_host_marker_write_route_exists() {
        let app = build_router(AppState::for_tests(Config::default()));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/api/plugin/items/item-1/markers")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"source":"tidb","markers":[{"markerType":"intro_start","startTicks":0}]}"#,
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("plugin host marker request should succeed");

        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn plugin_host_artwork_write_route_exists() {
        let app = build_router(AppState::for_tests(Config::default()));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/api/plugin/items/item-1/artwork")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"source":"tmdb","artwork":[{"artworkType":"poster","remoteUrl":"https://image.example.test/poster.jpg","width":1000,"height":1500,"isPrimary":true}]}"#,
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("plugin host artwork request should succeed");

        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn plugin_host_metadata_write_route_exists() {
        let app = build_router(AppState::for_tests(Config::default()));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::PATCH)
                    .uri("/api/plugin/items/item-1/metadata")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"title":"Movie","externalIds":[{"provider":"tmdb","externalId":"123"}],"genres":["Drama"],"tags":["Favorite"],"people":[{"name":"Jane Doe","roleType":"actor","roleName":"Lead","sortOrder":0}]}"#,
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("plugin host metadata request should succeed");

        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }
}
