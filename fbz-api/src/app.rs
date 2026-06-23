use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;
use tower_http::{
    cors::CorsLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

use crate::{admin, compat::emby, error::AppError, plugins, state::AppState};

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
    checks: ReadyChecks,
}

#[derive(Serialize)]
struct ReadyChecks {
    config: &'static str,
    database: &'static str,
    redis: &'static str,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .merge(admin::routes::router())
        .merge(plugins::routes::router())
        .merge(plugins::host::router())
        .merge(emby::routes::router())
        .fallback(not_found)
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "fbz-api",
        listen_addr: state.config().socket_addr().to_string(),
        node_role: state.config().node.role.as_str(),
    })
}

async fn ready(State(state): State<AppState>) -> Json<ReadyResponse> {
    Json(ReadyResponse {
        status: "ok",
        service: "fbz-api",
        checks: ReadyChecks {
            config: "ok",
            database: ready_label(state.database_ready()),
            redis: ready_label(state.redis_ready()),
        },
    })
}

fn ready_label(ready: bool) -> &'static str {
    if ready { "ok" } else { "not_configured" }
}

async fn not_found() -> AppError {
    AppError::not_found("route not found")
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode, header::CONTENT_TYPE},
    };
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;

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
    async fn ready_returns_ok() {
        let app = build_router(AppState::for_tests(Config::default()));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("ready request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
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
    async fn emby_display_preferences_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/DisplayPreferences/item-1?UserId=user-1&Client=Infuse",
            "/DisplayPreferences/item-1?UserId=user-1&Client=Infuse",
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
                .expect("display preferences request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
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
        ] {
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
                        .body(Body::from(r#"{"Username":"admin","Pw":"secret"}"#))
                        .expect("request should build"),
                )
                .await
                .expect("authenticate request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_logout_route_exists() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri) in [
            (Method::GET, "/emby/Auth/Providers"),
            (Method::GET, "/Auth/Providers"),
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
        }
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
    async fn emby_device_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for (method, uri, body) in [
            (Method::GET, "/emby/Devices", Body::empty()),
            (Method::GET, "/Devices", Body::empty()),
            (Method::DELETE, "/emby/Devices?Id=device-1", Body::empty()),
            (Method::DELETE, "/Devices?Id=device-1", Body::empty()),
            (Method::GET, "/emby/Devices/Info?Id=device-1", Body::empty()),
            (Method::GET, "/Devices/Info?Id=device-1", Body::empty()),
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
                Method::GET,
                "/emby/Devices/Options?Id=device-1",
                Body::empty(),
            ),
            (Method::GET, "/Devices/Options?Id=device-1", Body::empty()),
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
        }
    }

    #[tokio::test]
    async fn emby_user_views_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in ["/emby/Users/user-1/Views", "/Users/user-1/Views"] {
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
                .expect("views request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_library_media_folder_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Library/MediaFolders",
            "/Library/MediaFolders",
            "/emby/Library/SelectableMediaFolders",
            "/Library/SelectableMediaFolders",
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
            "/emby/Users/user-1/Items/Counts",
            "/Users/user-1/Items/Counts",
            "/emby/Users/user-1/Items/Root",
            "/Users/user-1/Items/Root",
            "/emby/Items/Counts?UserId=user-1",
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
        }
    }

    #[tokio::test]
    async fn emby_show_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Shows/NextUp",
            "/Shows/NextUp",
            "/emby/Shows/series-1/Seasons",
            "/Shows/series-1/Seasons",
            "/emby/Shows/series-1/Episodes",
            "/Shows/series-1/Episodes",
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
        }
    }

    #[tokio::test]
    async fn emby_genre_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Genres?UserId=user-1&Limit=20",
            "/Genres?UserId=user-1&Limit=20",
            "/emby/Genres/Action?UserId=user-1",
            "/Genres/Action?UserId=user-1",
            "/emby/MusicGenres?UserId=user-1&SearchTerm=rock",
            "/MusicGenres?UserId=user-1&SearchTerm=rock",
            "/emby/MusicGenres/Rock?UserId=user-1",
            "/MusicGenres/Rock?UserId=user-1",
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
        }
    }

    #[tokio::test]
    async fn emby_artist_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Artists?UserId=user-1&Limit=20",
            "/Artists?UserId=user-1&Limit=20",
            "/emby/Artists/AlbumArtists?UserId=user-1&SearchTerm=bow",
            "/Artists/AlbumArtists?UserId=user-1&SearchTerm=bow",
            "/emby/Artists/David%20Bowie?UserId=user-1",
            "/Artists/David%20Bowie?UserId=user-1",
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
        }
    }

    #[tokio::test]
    async fn emby_person_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Persons?UserId=user-1&PersonTypes=Actor,Director&Limit=20",
            "/Persons?UserId=user-1&PersonTypes=Actor,Director&Limit=20",
            "/emby/Persons/Tom%20Hanks?UserId=user-1",
            "/Persons/Tom%20Hanks?UserId=user-1",
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
    async fn emby_item_by_id_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Users/user-1/Items/item-1",
            "/Users/user-1/Items/item-1",
            "/emby/Items/item-1",
            "/Items/item-1",
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
        }
    }

    #[tokio::test]
    async fn emby_item_image_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/Images/Primary",
            "/Items/item-1/Images/Primary",
            "/emby/Items/item-1/Images/Backdrop/0",
            "/Items/item-1/Images/Backdrop/0",
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
                .expect("item image request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_remote_subtitle_search_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/RemoteSearch/Subtitles/eng?MediaSourceId=source-1&UserId=user-1",
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
        }
    }

    #[tokio::test]
    async fn emby_subtitle_stream_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/42/Subtitles/3/Stream.srt?StartPositionTicks=0",
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
    async fn emby_theme_media_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/ThemeMedia?UserId=user-1",
            "/Items/item-1/ThemeMedia?UserId=user-1",
            "/emby/Items/item-1/ThemeSongs?UserId=user-1",
            "/Items/item-1/ThemeSongs?UserId=user-1",
            "/emby/Items/item-1/ThemeVideos?UserId=user-1",
            "/Items/item-1/ThemeVideos?UserId=user-1",
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
    async fn emby_item_download_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Items/item-1/Download?MediaSourceId=42&api_key=test-token",
            "/Items/item-1/Download?MediaSourceId=42&api_key=test-token",
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
        }
    }

    #[tokio::test]
    async fn emby_hls_transcoding_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/videos/item-1/master.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/videos/item-1/master.m3u8?TranscodeSessionId=session-1&api_key=test-token",
            "/Videos/item-1/master.m3u8?TranscodeSessionId=session-1&api_key=test-token",
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
                .expect("hls transcoding request should succeed");

            assert_ne!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn emby_video_stream_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Videos/item-1/AdditionalParts?UserId=user-1&Fields=MediaSources",
            "/Videos/item-1/AdditionalParts?UserId=user-1&Fields=MediaSources",
            "/emby/Videos/item-1/stream?MediaSourceId=42&api_key=test-token",
            "/Videos/item-1/stream?MediaSourceId=42&api_key=test-token",
            "/emby/Videos/item-1/stream.mkv?MediaSourceId=42&api_key=test-token",
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
        }
    }

    #[tokio::test]
    async fn emby_audio_stream_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Audio/item-1/stream.mp3?MediaSourceId=42&api_key=test-token",
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
        }
    }

    #[tokio::test]
    async fn emby_playback_report_aliases_exist() {
        let app = build_router(AppState::for_tests(Config::default()));

        for uri in [
            "/emby/Sessions/Playing",
            "/Sessions/Playing",
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
            (Method::DELETE, "/Users/user-1/Items/item-1/Rating"),
            (
                Method::POST,
                "/emby/Users/user-1/Items/item-1/Rating/Delete",
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
