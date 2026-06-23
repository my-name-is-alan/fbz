use std::error::Error;

use fbz_api::{
    app::build_router,
    auth::bootstrap::ensure_bootstrap_admin,
    cache,
    config::{Config, NodeRole},
    db,
    events::worker::spawn_event_stream_mirror_worker,
    media::{probe::spawn_probe_worker, tools::resolve_media_tools},
    metadata::worker::spawn_metadata_worker,
    notifications::worker::spawn_notification_worker,
    plugins::worker::spawn_plugin_worker,
    scan::worker::spawn_scan_worker,
    scheduler::{service::SchedulerService, worker::spawn_scheduler_worker},
    settings::{bootstrap_settings, repository::SettingsRepository},
    state::AppState,
    telemetry::init_tracing,
    transcode::worker::spawn_transcode_worker,
};
use tokio::{net::TcpListener, sync::broadcast};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::from_env()?;
    init_tracing(&config.telemetry);
    let media_tools = resolve_media_tools(&config.media_tools)?;
    info!(
        ffmpeg_path = %media_tools.ffmpeg.path.display(),
        ffmpeg_source = ?media_tools.ffmpeg.source,
        ffmpeg_version = %media_tools.ffmpeg.version_line,
        ffprobe_path = %media_tools.ffprobe.path.display(),
        ffprobe_source = ?media_tools.ffprobe.source,
        ffprobe_version = %media_tools.ffprobe.version_line,
        "media tools resolved"
    );

    let database = db::connect(&config.database).await?;
    db::migrate(&database).await?;
    let settings_repository = SettingsRepository::new(database.clone());
    let bootstrap_settings = bootstrap_settings(&config)?;
    settings_repository
        .insert_bootstrap_defaults(&bootstrap_settings)
        .await?;
    SchedulerService::new(database.clone())
        .bootstrap_core_tasks(&config.scheduler, &config.schedules)
        .await?;
    let settings_count = settings_repository.list().await?.len();
    let bootstrap_admin = ensure_bootstrap_admin(&database, &config.bootstrap_admin).await?;
    info!(
        settings_count,
        bootstrap_admin = ?bootstrap_admin,
        "database connected and runtime settings initialized"
    );

    let mut redis = cache::connect(&config.redis).await?;
    let redis_ping = cache::ping(&mut redis).await?;
    info!(redis_ping, "redis connected");

    let (shutdown_tx, _) = broadcast::channel(4);
    let event_stream_mirror_worker = if should_start_event_stream_mirror_worker(&config) {
        Some(spawn_event_stream_mirror_worker(
            database.clone(),
            redis.clone(),
            config.redis.clone(),
            shutdown_tx.subscribe(),
        ))
    } else {
        info!(
            event_streams_enabled = config.redis.event_streams_enabled,
            node_role = config.node.role.as_str(),
            "event stream mirror worker not started"
        );
        None
    };
    let scan_worker = if should_start_scan_worker(&config) {
        Some(spawn_scan_worker(
            database.clone(),
            config.scan_worker.clone(),
            shutdown_tx.subscribe(),
        ))
    } else {
        info!(
            scan_worker_enabled = config.scan_worker.enabled,
            node_role = config.node.role.as_str(),
            "scan worker not started"
        );
        None
    };
    let scheduler_worker = if should_start_scheduler_worker(&config) {
        Some(spawn_scheduler_worker(
            database.clone(),
            config.scheduler.clone(),
            shutdown_tx.subscribe(),
        ))
    } else {
        info!(
            scheduler_enabled = config.scheduler.enabled,
            node_role = config.node.role.as_str(),
            "scheduler worker not started"
        );
        None
    };
    let plugin_worker = if should_start_plugin_worker(&config) {
        Some(spawn_plugin_worker(
            database.clone(),
            config.plugins.clone(),
            config.server.public_base_url.clone(),
            config.plugin_worker.clone(),
            shutdown_tx.subscribe(),
        ))
    } else {
        info!(
            plugin_worker_enabled = config.plugin_worker.enabled,
            node_role = config.node.role.as_str(),
            "plugin worker not started"
        );
        None
    };
    let transcode_worker = if should_start_transcode_worker(&config) {
        Some(spawn_transcode_worker(
            database.clone(),
            config.transcode.clone(),
            config.transcode_worker.clone(),
            media_tools.clone(),
            shutdown_tx.subscribe(),
        ))
    } else {
        info!(
            transcode_worker_enabled = config.transcode_worker.enabled,
            node_role = config.node.role.as_str(),
            "transcode worker not started"
        );
        None
    };
    let probe_worker = if should_start_probe_worker(&config) {
        Some(spawn_probe_worker(
            database.clone(),
            config.probe_worker.clone(),
            media_tools.clone(),
            shutdown_tx.subscribe(),
        ))
    } else {
        info!(
            probe_worker_enabled = config.probe_worker.enabled,
            node_role = config.node.role.as_str(),
            "probe worker not started"
        );
        None
    };
    let metadata_worker = if should_start_metadata_worker(&config) {
        Some(spawn_metadata_worker(
            database.clone(),
            config.metadata.clone(),
            config.proxy.clone(),
            config.metadata_worker.clone(),
            shutdown_tx.subscribe(),
        ))
    } else {
        info!(
            metadata_worker_enabled = config.metadata_worker.enabled,
            node_role = config.node.role.as_str(),
            "metadata worker not started"
        );
        None
    };
    let notification_worker = if should_start_notification_worker(&config) {
        Some(spawn_notification_worker(
            database.clone(),
            config.notification_worker.clone(),
            config.secrets.clone(),
            shutdown_tx.subscribe(),
        ))
    } else {
        info!(
            notification_worker_enabled = config.notification_worker.enabled,
            node_role = config.node.role.as_str(),
            "notification worker not started"
        );
        None
    };

    let addr = config.socket_addr();
    let app = build_router(AppState::new(config, database, redis));
    let listener = TcpListener::bind(addr).await?;

    info!(%addr, "fbz-api listening");

    let serve_result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_tx.clone()))
        .await;

    let _ = shutdown_tx.send(());
    if let Some(scan_worker) = scan_worker {
        if let Err(err) = scan_worker.await {
            warn!(error = %err, "scan worker join failed");
        }
    }
    if let Some(scheduler_worker) = scheduler_worker {
        if let Err(err) = scheduler_worker.await {
            warn!(error = %err, "scheduler worker join failed");
        }
    }
    if let Some(plugin_worker) = plugin_worker {
        if let Err(err) = plugin_worker.await {
            warn!(error = %err, "plugin worker join failed");
        }
    }
    if let Some(transcode_worker) = transcode_worker {
        if let Err(err) = transcode_worker.await {
            warn!(error = %err, "transcode worker join failed");
        }
    }
    if let Some(probe_worker) = probe_worker {
        if let Err(err) = probe_worker.await {
            warn!(error = %err, "probe worker join failed");
        }
    }
    if let Some(metadata_worker) = metadata_worker {
        if let Err(err) = metadata_worker.await {
            warn!(error = %err, "metadata worker join failed");
        }
    }
    if let Some(notification_worker) = notification_worker {
        if let Err(err) = notification_worker.await {
            warn!(error = %err, "notification worker join failed");
        }
    }
    if let Some(event_stream_mirror_worker) = event_stream_mirror_worker {
        if let Err(err) = event_stream_mirror_worker.await {
            warn!(error = %err, "event stream mirror worker join failed");
        }
    }

    serve_result?;

    Ok(())
}

fn should_start_scan_worker(config: &Config) -> bool {
    config.scan_worker.enabled && matches!(config.node.role, NodeRole::All | NodeRole::Worker)
}

fn should_start_scheduler_worker(config: &Config) -> bool {
    config.scheduler.enabled && matches!(config.node.role, NodeRole::All | NodeRole::Scheduler)
}

fn should_start_plugin_worker(config: &Config) -> bool {
    config.plugin_worker.enabled && matches!(config.node.role, NodeRole::All | NodeRole::Worker)
}

fn should_start_transcode_worker(config: &Config) -> bool {
    config.transcode_worker.enabled && matches!(config.node.role, NodeRole::All | NodeRole::Worker)
}

fn should_start_probe_worker(config: &Config) -> bool {
    config.probe_worker.enabled && matches!(config.node.role, NodeRole::All | NodeRole::Worker)
}

fn should_start_metadata_worker(config: &Config) -> bool {
    config.metadata_worker.enabled && matches!(config.node.role, NodeRole::All | NodeRole::Worker)
}

fn should_start_notification_worker(config: &Config) -> bool {
    config.notification_worker.enabled
        && matches!(config.node.role, NodeRole::All | NodeRole::Worker)
}

fn should_start_event_stream_mirror_worker(config: &Config) -> bool {
    config.redis.event_streams_enabled
        && matches!(config.node.role, NodeRole::All | NodeRole::Worker)
}

async fn shutdown_signal(shutdown_tx: broadcast::Sender<()>) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("shutdown signal received");
    let _ = shutdown_tx.send(());
}
