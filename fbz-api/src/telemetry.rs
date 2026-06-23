use tracing_subscriber::EnvFilter;

use crate::config::TelemetryConfig;

pub fn init_tracing(config: &TelemetryConfig) {
    let filter = EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .compact()
        .init();
}
