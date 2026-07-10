use std::{
    env,
    error::Error,
    fmt::{Display, Formatter},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    str::FromStr,
};

const DEFAULT_HOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const DEFAULT_PORT: u16 = 8080;
const DEFAULT_READINESS_TIMEOUT_MS: u64 = 500;
const MAX_READINESS_TIMEOUT_MS: u64 = 60_000;
const DEFAULT_HTTP_SLOW_LOG_THRESHOLD_MS: u64 = 1_000;
const DEFAULT_LOG_LEVEL: &str = "fbz_api=info,tower_http=info";
const DEFAULT_DATABASE_URL: &str = "postgres://fbz:fbz@127.0.0.1:5432/fbz";
const DEFAULT_DATABASE_ACQUIRE_TIMEOUT_SECONDS: u64 = 5;
const DEFAULT_DATABASE_IDLE_TIMEOUT_SECONDS: u64 = 600;
const DEFAULT_DATABASE_MAX_LIFETIME_SECONDS: u64 = 1_800;
const DEFAULT_DATABASE_STATEMENT_TIMEOUT_MS: u32 = 30_000;
const DEFAULT_DATABASE_SLOW_LOG_THRESHOLD_MS: u64 = 1_000;
const DEFAULT_REDIS_URL: &str = "redis://127.0.0.1:6379";
const DEFAULT_REDIS_EVENT_STREAM_KEY: &str = "fbz:events";
const DEFAULT_REDIS_EVENT_STREAM_MAX_LEN: u64 = 50_000;
const DEFAULT_REDIS_EVENT_STREAM_BATCH_SIZE: u16 = 100;
const DEFAULT_REDIS_EVENT_STREAM_INTERVAL_SECONDS: u64 = 5;
const DEFAULT_REDIS_EVENT_STREAM_LEASE_SECONDS: u64 = 30;
const DEFAULT_REDIS_EVENT_STREAM_RETRY_BASE_SECONDS: u64 = 5;
const DEFAULT_REDIS_EVENT_STREAM_RETRY_MAX_SECONDS: u64 = 300;
const DEFAULT_REDIS_OPERATION_TIMEOUT_MS: u64 = 2_000;
const MAX_REDIS_OPERATION_TIMEOUT_MS: u64 = 60_000;
const DEFAULT_PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES: usize = 64 * 1024;
const MAX_PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES: usize = 1024 * 1024;
const DEFAULT_PLUGIN_WASI_FUEL: u64 = 100_000_000;
const DEFAULT_PLUGIN_WASI_STDIO_MAX_BYTES: usize = 64 * 1024;
const MAX_PLUGIN_WASI_STDIO_MAX_BYTES: usize = 1024 * 1024;
const DEFAULT_PLUGIN_WASI_MAX_MODULE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PLUGIN_WASI_MAX_MODULE_BYTES: u64 = 256 * 1024 * 1024;
const DEFAULT_PLUGIN_TMP_MAX_AGE_SECONDS: u64 = 24 * 60 * 60;
const DEFAULT_PLUGIN_HOST_API_MAX_CALLS_PER_RUN: u32 = 10_000;
const MAX_PLUGIN_HOST_API_MAX_CALLS_PER_RUN: u32 = 100_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub server: ServerConfig,
    pub node: NodeConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub metadata: MetadataConfig,
    pub proxy: ProxyConfig,
    pub media_tools: MediaToolConfig,
    pub media: MediaConfig,
    pub storage: StorageConfig,
    pub secrets: SecretConfig,
    pub transcode: TranscodeConfig,
    pub plugins: PluginConfig,
    pub schedules: ScheduleConfig,
    pub scan_worker: ScanWorkerConfig,
    pub scheduler: SchedulerWorkerConfig,
    pub transcode_worker: TranscodeWorkerConfig,
    pub probe_worker: ProbeWorkerConfig,
    pub photo_worker: PhotoWorkerConfig,
    pub metadata_worker: MetadataWorkerConfig,
    pub plugin_worker: PluginWorkerConfig,
    pub notification_worker: NotificationWorkerConfig,
    pub bootstrap_admin: BootstrapAdminConfig,
    pub telemetry: TelemetryConfig,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerConfig {
    pub host: IpAddr,
    pub port: u16,
    pub readiness_timeout_ms: u64,
    pub public_base_url: String,
    pub public_base_url_admin_editable: bool,
}

impl ServerConfig {
    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeConfig {
    pub role: NodeRole,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeRole {
    All,
    Api,
    Worker,
    Scheduler,
}

impl NodeRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Api => "api",
            Self::Worker => "worker",
            Self::Scheduler => "scheduler",
        }
    }
}

impl FromStr for NodeRole {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "all" => Ok(Self::All),
            "api" => Ok(Self::Api),
            "worker" => Ok(Self::Worker),
            "scheduler" => Ok(Self::Scheduler),
            other => Err(format!(
                "unsupported node role `{other}`, expected all, api, worker, or scheduler"
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseConfig {
    pub url: String,
    pub min_connections: u32,
    pub max_connections: u32,
    pub acquire_timeout_seconds: u64,
    pub idle_timeout_seconds: u64,
    pub max_lifetime_seconds: u64,
    pub statement_timeout_ms: u32,
    pub slow_log_threshold_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedisConfig {
    pub url: String,
    pub operation_timeout_ms: u64,
    pub event_streams_enabled: bool,
    pub event_stream_key: String,
    pub event_stream_max_len: u64,
    pub event_stream_batch_size: u16,
    pub event_stream_interval_seconds: u64,
    pub event_stream_lease_seconds: u64,
    pub event_stream_retry_base_seconds: u64,
    pub event_stream_retry_max_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataConfig {
    pub providers: Vec<String>,
    pub tmdb_access_token: Option<String>,
    pub tmdb_api_base_url: String,
    pub tmdb_image_base_url: String,
    pub tvdb_api_key: Option<String>,
    pub tvdb_api_base_url: String,
    pub fanart_api_key: Option<String>,
    pub fanart_api_base_url: String,
    /// Spotify Web API client credentials (默认音乐查询 provider). 缺省时 spotify provider 跳过。
    pub spotify_client_id: Option<String>,
    pub spotify_client_secret: Option<String>,
    pub spotify_api_base_url: String,
    pub spotify_auth_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxyConfig {
    pub http_proxy: Option<String>,
    pub https_proxy: Option<String>,
    pub no_proxy: Vec<String>,
    pub policy: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaToolConfig {
    pub ffmpeg_path: String,
    pub ffmpeg_path_explicit: bool,
    pub ffprobe_path: String,
    pub ffprobe_path_explicit: bool,
    pub bundled_dir: PathBuf,
    pub enable_bundled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaConfig {
    pub roots: Vec<PathBuf>,
    pub strm_allow_private_networks: bool,
    pub strm_allowed_domains: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageConfig {
    pub backend: String,
    pub transcode_cache_dir: PathBuf,
    pub artwork_cache_dir: PathBuf,
    pub scan_event_retention_days: u16,
    /// 相机上传落盘目录（`Devices/CameraUploads`），按设备分子目录。
    pub camera_upload_dir: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretConfig {
    pub key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscodeConfig {
    pub max_concurrent: u16,
    pub lease_seconds: u64,
    pub hardware_mode: HardwareMode,
    pub hardware_priority: Vec<String>,
    pub software_fallback: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HardwareMode {
    Auto,
    HardwareOnly,
    SoftwareOnly,
    Disabled,
}

impl HardwareMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::HardwareOnly => "hardware",
            Self::SoftwareOnly => "software",
            Self::Disabled => "disabled",
        }
    }
}

impl FromStr for HardwareMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "hardware" | "hardware-only" | "hw" => Ok(Self::HardwareOnly),
            "software" | "software-only" | "sw" => Ok(Self::SoftwareOnly),
            "disabled" | "off" => Ok(Self::Disabled),
            other => Err(format!(
                "unsupported hardware mode `{other}`, expected auto, hardware, software, or disabled"
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginConfig {
    pub dir: PathBuf,
    pub package_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub tmp_max_age_seconds: u64,
    pub runtime_default: String,
    pub require_approval: bool,
    pub require_reapproval_on_permission_change: bool,
    pub allow_unsigned: bool,
    pub trusted_signature_keys: Vec<PluginTrustedSignatureKey>,
    pub timeout_ms: u64,
    pub max_concurrency: u16,
    pub memory_limit_mb: u16,
    pub wasi_fuel: u64,
    pub wasi_stdio_max_bytes: usize,
    pub wasi_max_module_bytes: u64,
    pub http_max_response_body_bytes: usize,
    pub host_api_max_calls_per_run: u32,
    pub secret_key: Option<String>,
    pub http_allowed_hosts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginTrustedSignatureKey {
    pub key_id: String,
    pub public_key: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleConfig {
    pub incremental_scan: String,
    pub full_scan: String,
    pub metadata_refresh: String,
    pub transcode_cleanup: String,
    pub session_cleanup: String,
    pub partition_maintenance: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanWorkerConfig {
    pub enabled: bool,
    pub interval_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchedulerWorkerConfig {
    pub enabled: bool,
    pub interval_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscodeWorkerConfig {
    pub enabled: bool,
    pub interval_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeWorkerConfig {
    pub enabled: bool,
    pub interval_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhotoWorkerConfig {
    pub enabled: bool,
    pub interval_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataWorkerConfig {
    pub enabled: bool,
    pub interval_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginWorkerConfig {
    pub enabled: bool,
    pub interval_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationWorkerConfig {
    pub enabled: bool,
    pub interval_seconds: u64,
    pub delivery_timeout_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapAdminConfig {
    pub username: Option<String>,
    pub password: Option<String>,
}

/// 管理员密码最小长度（位）。env bootstrap、HTTP `POST /api/setup`、`POST /api/admin/users`
/// 三条建管理员/用户通道共用此策略，避免规则漂移。
pub const ADMIN_PASSWORD_MIN_LEN: usize = 6;

/// 管理员/用户密码是否满足强度策略（当前仅长度下限）。
pub fn admin_password_meets_policy(password: &str) -> bool {
    password.len() >= ADMIN_PASSWORD_MIN_LEN
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TelemetryConfig {
    pub log_level: String,
    pub http_slow_log_threshold_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    key: &'static str,
    message: String,
}

impl ConfigError {
    fn new(key: &'static str, message: impl Into<String>) -> Self {
        Self {
            key,
            message: message.into(),
        }
    }

    #[cfg(test)]
    fn key(&self) -> &'static str {
        self.key
    }
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid config {}: {}", self.key, self.message)
    }
}

impl Error for ConfigError {}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_source(|key| env::var(key).ok())
    }

    pub fn from_source(source: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let host = parse_or("FBZ_API_HOST", DEFAULT_HOST, &source)?;
        let port = parse_or("FBZ_API_PORT", DEFAULT_PORT, &source)?;
        let public_base_url = get_or_else("PUBLIC_BASE_URL", &source, || {
            format!("http://{host}:{port}")
        });

        let config = Self {
            server: ServerConfig {
                host,
                port,
                readiness_timeout_ms: parse_or(
                    "FBZ_READINESS_TIMEOUT_MS",
                    DEFAULT_READINESS_TIMEOUT_MS,
                    &source,
                )?,
                public_base_url,
                public_base_url_admin_editable: bool_or(
                    "PUBLIC_BASE_URL_ADMIN_EDITABLE",
                    true,
                    &source,
                )?,
            },
            node: NodeConfig {
                role: parse_or("FBZ_NODE_ROLE", NodeRole::All, &source)?,
            },
            database: DatabaseConfig {
                url: get_or("DATABASE_URL", DEFAULT_DATABASE_URL, &source),
                min_connections: parse_or("DATABASE_MIN_CONNECTIONS", 1_u32, &source)?,
                max_connections: parse_or("DATABASE_MAX_CONNECTIONS", 20_u32, &source)?,
                acquire_timeout_seconds: parse_or(
                    "DATABASE_ACQUIRE_TIMEOUT_SECONDS",
                    DEFAULT_DATABASE_ACQUIRE_TIMEOUT_SECONDS,
                    &source,
                )?,
                idle_timeout_seconds: parse_or(
                    "DATABASE_IDLE_TIMEOUT_SECONDS",
                    DEFAULT_DATABASE_IDLE_TIMEOUT_SECONDS,
                    &source,
                )?,
                max_lifetime_seconds: parse_or(
                    "DATABASE_MAX_LIFETIME_SECONDS",
                    DEFAULT_DATABASE_MAX_LIFETIME_SECONDS,
                    &source,
                )?,
                statement_timeout_ms: parse_or(
                    "DATABASE_STATEMENT_TIMEOUT_MS",
                    DEFAULT_DATABASE_STATEMENT_TIMEOUT_MS,
                    &source,
                )?,
                slow_log_threshold_ms: parse_or(
                    "DATABASE_SLOW_LOG_THRESHOLD_MS",
                    DEFAULT_DATABASE_SLOW_LOG_THRESHOLD_MS,
                    &source,
                )?,
            },
            redis: RedisConfig {
                url: get_or("REDIS_URL", DEFAULT_REDIS_URL, &source),
                operation_timeout_ms: parse_or(
                    "REDIS_OPERATION_TIMEOUT_MS",
                    DEFAULT_REDIS_OPERATION_TIMEOUT_MS,
                    &source,
                )?,
                event_streams_enabled: bool_or("REDIS_EVENT_STREAMS_ENABLED", false, &source)?,
                event_stream_key: get_or(
                    "REDIS_EVENT_STREAM_KEY",
                    DEFAULT_REDIS_EVENT_STREAM_KEY,
                    &source,
                ),
                event_stream_max_len: parse_or(
                    "REDIS_EVENT_STREAM_MAX_LEN",
                    DEFAULT_REDIS_EVENT_STREAM_MAX_LEN,
                    &source,
                )?,
                event_stream_batch_size: parse_or(
                    "REDIS_EVENT_STREAM_BATCH_SIZE",
                    DEFAULT_REDIS_EVENT_STREAM_BATCH_SIZE,
                    &source,
                )?,
                event_stream_interval_seconds: parse_or(
                    "REDIS_EVENT_STREAM_INTERVAL_SECONDS",
                    DEFAULT_REDIS_EVENT_STREAM_INTERVAL_SECONDS,
                    &source,
                )?,
                event_stream_lease_seconds: parse_or(
                    "REDIS_EVENT_STREAM_LEASE_SECONDS",
                    DEFAULT_REDIS_EVENT_STREAM_LEASE_SECONDS,
                    &source,
                )?,
                event_stream_retry_base_seconds: parse_or(
                    "REDIS_EVENT_STREAM_RETRY_BASE_SECONDS",
                    DEFAULT_REDIS_EVENT_STREAM_RETRY_BASE_SECONDS,
                    &source,
                )?,
                event_stream_retry_max_seconds: parse_or(
                    "REDIS_EVENT_STREAM_RETRY_MAX_SECONDS",
                    DEFAULT_REDIS_EVENT_STREAM_RETRY_MAX_SECONDS,
                    &source,
                )?,
            },
            metadata: MetadataConfig {
                providers: csv_or("METADATA_PROVIDERS", "tmdb,tvdb,fanart", &source),
                tmdb_access_token: optional("TMDB_ACCESS_TOKEN", &source),
                tmdb_api_base_url: get_or(
                    "TMDB_API_BASE_URL",
                    "https://api.themoviedb.org/3",
                    &source,
                ),
                tmdb_image_base_url: get_or(
                    "TMDB_IMAGE_BASE_URL",
                    "https://image.tmdb.org/t/p",
                    &source,
                ),
                tvdb_api_key: optional("TVDB_API_KEY", &source),
                tvdb_api_base_url: get_or(
                    "TVDB_API_BASE_URL",
                    "https://api4.thetvdb.com/v4",
                    &source,
                ),
                fanart_api_key: optional("FANART_API_KEY", &source),
                fanart_api_base_url: get_or(
                    "FANART_API_BASE_URL",
                    "https://webservice.fanart.tv/v3",
                    &source,
                ),
                spotify_client_id: optional("SPOTIFY_CLIENT_ID", &source),
                spotify_client_secret: optional("SPOTIFY_CLIENT_SECRET", &source),
                spotify_api_base_url: get_or(
                    "SPOTIFY_API_BASE_URL",
                    "https://api.spotify.com/v1",
                    &source,
                ),
                spotify_auth_url: get_or(
                    "SPOTIFY_AUTH_URL",
                    "https://accounts.spotify.com/api/token",
                    &source,
                ),
            },
            proxy: ProxyConfig {
                http_proxy: optional("HTTP_PROXY", &source),
                https_proxy: optional("HTTPS_PROXY", &source),
                no_proxy: csv_or("NO_PROXY", "127.0.0.1,localhost", &source),
                policy: get_or("PROXY_POLICY", "global-with-provider-override", &source),
            },
            media_tools: MediaToolConfig {
                ffmpeg_path: get_or("FFMPEG_PATH", "ffmpeg", &source),
                ffmpeg_path_explicit: optional("FFMPEG_PATH", &source).is_some(),
                ffprobe_path: get_or("FFPROBE_PATH", "ffprobe", &source),
                ffprobe_path_explicit: optional("FFPROBE_PATH", &source).is_some(),
                bundled_dir: path_or("FBZ_BUNDLED_FFMPEG_DIR", "./vendor/ffmpeg", &source),
                enable_bundled: bool_or("FBZ_ENABLE_BUNDLED_FFMPEG", true, &source)?,
            },
            media: MediaConfig {
                roots: paths_or(
                    "MEDIA_ROOTS",
                    "D:/Media/Movies,D:/Media/TV,D:/Media/Music",
                    &source,
                ),
                strm_allow_private_networks: bool_or("STRM_ALLOW_PRIVATE_NETWORKS", true, &source)?,
                strm_allowed_domains: csv_or("STRM_ALLOWED_DOMAINS", "", &source),
            },
            storage: StorageConfig {
                backend: get_or("STORAGE_BACKEND", "filesystem", &source),
                transcode_cache_dir: path_or("TRANSCODE_CACHE_DIR", "./var/transcode", &source),
                artwork_cache_dir: path_or("ARTWORK_CACHE_DIR", "./var/artwork", &source),
                scan_event_retention_days: parse_or("SCAN_EVENT_RETENTION_DAYS", 90_u16, &source)?,
                camera_upload_dir: path_or("CAMERA_UPLOAD_DIR", "./var/camera-uploads", &source),
            },
            secrets: SecretConfig {
                key: optional("FBZ_SECRET_KEY", &source),
            },
            transcode: TranscodeConfig {
                max_concurrent: parse_or("TRANSCODE_MAX_CONCURRENT", 3_u16, &source)?,
                lease_seconds: parse_or("TRANSCODE_LEASE_SECONDS", 900_u64, &source)?,
                hardware_mode: parse_or("TRANSCODE_HARDWARE_MODE", HardwareMode::Auto, &source)?,
                hardware_priority: csv_or(
                    "TRANSCODE_HARDWARE_PRIORITY",
                    "intel,nvidia,amd",
                    &source,
                ),
                software_fallback: bool_or("TRANSCODE_SOFTWARE_FALLBACK", true, &source)?,
            },
            plugins: PluginConfig {
                dir: path_or("PLUGIN_DIR", "./plugins", &source),
                package_dir: path_or("PLUGIN_PACKAGE_DIR", "./var/plugin-packages", &source),
                data_dir: path_or("PLUGIN_DATA_DIR", "./var/plugin-data", &source),
                cache_dir: path_or("PLUGIN_CACHE_DIR", "./var/plugin-cache", &source),
                tmp_dir: path_or("PLUGIN_TMP_DIR", "./var/plugin-tmp", &source),
                tmp_max_age_seconds: parse_or(
                    "PLUGIN_TMP_MAX_AGE_SECONDS",
                    DEFAULT_PLUGIN_TMP_MAX_AGE_SECONDS,
                    &source,
                )?,
                runtime_default: get_or("PLUGIN_RUNTIME_DEFAULT", "wasi", &source),
                require_approval: bool_or("PLUGIN_REQUIRE_APPROVAL", true, &source)?,
                require_reapproval_on_permission_change: bool_or(
                    "PLUGIN_REQUIRE_REAPPROVAL_ON_PERMISSION_CHANGE",
                    true,
                    &source,
                )?,
                allow_unsigned: bool_or("PLUGIN_ALLOW_UNSIGNED", false, &source)?,
                trusted_signature_keys: plugin_trusted_signature_keys_or(
                    "PLUGIN_TRUSTED_SIGNATURE_KEYS",
                    &source,
                )?,
                timeout_ms: parse_or("PLUGIN_TIMEOUT_MS", 5_000_u64, &source)?,
                max_concurrency: parse_or("PLUGIN_MAX_CONCURRENCY", 4_u16, &source)?,
                memory_limit_mb: parse_or("PLUGIN_MEMORY_LIMIT_MB", 128_u16, &source)?,
                wasi_fuel: parse_or("PLUGIN_WASI_FUEL", DEFAULT_PLUGIN_WASI_FUEL, &source)?,
                wasi_stdio_max_bytes: parse_or(
                    "PLUGIN_WASI_STDIO_MAX_BYTES",
                    DEFAULT_PLUGIN_WASI_STDIO_MAX_BYTES,
                    &source,
                )?,
                wasi_max_module_bytes: parse_or(
                    "PLUGIN_WASI_MAX_MODULE_BYTES",
                    DEFAULT_PLUGIN_WASI_MAX_MODULE_BYTES,
                    &source,
                )?,
                http_max_response_body_bytes: parse_or(
                    "PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES",
                    DEFAULT_PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES,
                    &source,
                )?,
                host_api_max_calls_per_run: parse_or(
                    "PLUGIN_HOST_API_MAX_CALLS_PER_RUN",
                    DEFAULT_PLUGIN_HOST_API_MAX_CALLS_PER_RUN,
                    &source,
                )?,
                secret_key: optional("PLUGIN_SECRET_KEY", &source),
                http_allowed_hosts: csv_or(
                    "PLUGIN_HTTP_ALLOWED_HOSTS",
                    "127.0.0.1,localhost,::1,host.docker.internal",
                    &source,
                ),
            },
            schedules: ScheduleConfig {
                incremental_scan: get_or("SCHEDULE_INCREMENTAL_SCAN", "15m", &source),
                full_scan: get_or("SCHEDULE_FULL_SCAN", "0 4 * * *", &source),
                metadata_refresh: get_or("SCHEDULE_METADATA_REFRESH", "0 5 * * *", &source),
                transcode_cleanup: get_or("SCHEDULE_TRANSCODE_CLEANUP", "hourly", &source),
                session_cleanup: get_or("SCHEDULE_SESSION_CLEANUP", "10m", &source),
                partition_maintenance: get_or("SCHEDULE_PARTITION_MAINTENANCE", "daily", &source),
            },
            scan_worker: ScanWorkerConfig {
                enabled: bool_or("FBZ_SCAN_WORKER_ENABLED", false, &source)?,
                interval_seconds: parse_or("FBZ_SCAN_WORKER_INTERVAL_SECONDS", 5_u64, &source)?,
            },
            scheduler: SchedulerWorkerConfig {
                enabled: bool_or("FBZ_SCHEDULER_ENABLED", false, &source)?,
                interval_seconds: parse_or("FBZ_SCHEDULER_INTERVAL_SECONDS", 5_u64, &source)?,
            },
            transcode_worker: TranscodeWorkerConfig {
                enabled: bool_or("FBZ_TRANSCODE_WORKER_ENABLED", false, &source)?,
                interval_seconds: parse_or(
                    "FBZ_TRANSCODE_WORKER_INTERVAL_SECONDS",
                    5_u64,
                    &source,
                )?,
            },
            probe_worker: ProbeWorkerConfig {
                enabled: bool_or("FBZ_PROBE_WORKER_ENABLED", false, &source)?,
                interval_seconds: parse_or("FBZ_PROBE_WORKER_INTERVAL_SECONDS", 10_u64, &source)?,
            },
            photo_worker: PhotoWorkerConfig {
                enabled: bool_or("FBZ_PHOTO_WORKER_ENABLED", false, &source)?,
                interval_seconds: parse_or("FBZ_PHOTO_WORKER_INTERVAL_SECONDS", 10_u64, &source)?,
            },
            metadata_worker: MetadataWorkerConfig {
                enabled: bool_or("FBZ_METADATA_WORKER_ENABLED", false, &source)?,
                interval_seconds: parse_or(
                    "FBZ_METADATA_WORKER_INTERVAL_SECONDS",
                    10_u64,
                    &source,
                )?,
            },
            plugin_worker: PluginWorkerConfig {
                enabled: bool_or("FBZ_PLUGIN_WORKER_ENABLED", false, &source)?,
                interval_seconds: parse_or("FBZ_PLUGIN_WORKER_INTERVAL_SECONDS", 5_u64, &source)?,
            },
            notification_worker: NotificationWorkerConfig {
                enabled: bool_or("FBZ_NOTIFICATION_WORKER_ENABLED", false, &source)?,
                interval_seconds: parse_or(
                    "FBZ_NOTIFICATION_WORKER_INTERVAL_SECONDS",
                    5_u64,
                    &source,
                )?,
                delivery_timeout_ms: parse_or(
                    "FBZ_NOTIFICATION_DELIVERY_TIMEOUT_MS",
                    5_000_u64,
                    &source,
                )?,
            },
            bootstrap_admin: BootstrapAdminConfig {
                username: optional("FBZ_BOOTSTRAP_ADMIN_USERNAME", &source),
                password: optional("FBZ_BOOTSTRAP_ADMIN_PASSWORD", &source),
            },
            telemetry: TelemetryConfig {
                log_level: get_or("RUST_LOG", DEFAULT_LOG_LEVEL, &source),
                http_slow_log_threshold_ms: parse_or(
                    "HTTP_SLOW_LOG_THRESHOLD_MS",
                    DEFAULT_HTTP_SLOW_LOG_THRESHOLD_MS,
                    &source,
                )?,
            },
        };

        config.validate()?;
        Ok(config)
    }

    pub fn socket_addr(&self) -> SocketAddr {
        self.server.socket_addr()
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        require_url("PUBLIC_BASE_URL", &self.server.public_base_url)?;
        if self.server.readiness_timeout_ms == 0 {
            return Err(ConfigError::new(
                "FBZ_READINESS_TIMEOUT_MS",
                "must be greater than zero",
            ));
        }
        if self.server.readiness_timeout_ms > MAX_READINESS_TIMEOUT_MS {
            return Err(ConfigError::new(
                "FBZ_READINESS_TIMEOUT_MS",
                "must be less than or equal to 60000",
            ));
        }
        if self.telemetry.http_slow_log_threshold_ms == 0 {
            return Err(ConfigError::new(
                "HTTP_SLOW_LOG_THRESHOLD_MS",
                "must be greater than zero",
            ));
        }

        if !self.database.url.starts_with("postgres://")
            && !self.database.url.starts_with("postgresql://")
        {
            return Err(ConfigError::new(
                "DATABASE_URL",
                "must start with postgres:// or postgresql://",
            ));
        }

        if self.database.max_connections == 0 {
            return Err(ConfigError::new(
                "DATABASE_MAX_CONNECTIONS",
                "must be greater than zero",
            ));
        }
        if self.database.min_connections > self.database.max_connections {
            return Err(ConfigError::new(
                "DATABASE_MIN_CONNECTIONS",
                "must be less than or equal to DATABASE_MAX_CONNECTIONS",
            ));
        }
        if self.database.acquire_timeout_seconds == 0 {
            return Err(ConfigError::new(
                "DATABASE_ACQUIRE_TIMEOUT_SECONDS",
                "must be greater than zero",
            ));
        }
        if self.database.idle_timeout_seconds == 0 {
            return Err(ConfigError::new(
                "DATABASE_IDLE_TIMEOUT_SECONDS",
                "must be greater than zero",
            ));
        }
        if self.database.max_lifetime_seconds == 0 {
            return Err(ConfigError::new(
                "DATABASE_MAX_LIFETIME_SECONDS",
                "must be greater than zero",
            ));
        }
        if self.database.statement_timeout_ms == 0 {
            return Err(ConfigError::new(
                "DATABASE_STATEMENT_TIMEOUT_MS",
                "must be greater than zero",
            ));
        }
        if self.database.slow_log_threshold_ms == 0 {
            return Err(ConfigError::new(
                "DATABASE_SLOW_LOG_THRESHOLD_MS",
                "must be greater than zero",
            ));
        }

        if !self.redis.url.starts_with("redis://") && !self.redis.url.starts_with("rediss://") {
            return Err(ConfigError::new(
                "REDIS_URL",
                "must start with redis:// or rediss://",
            ));
        }
        if self.redis.operation_timeout_ms == 0 {
            return Err(ConfigError::new(
                "REDIS_OPERATION_TIMEOUT_MS",
                "must be greater than zero",
            ));
        }
        if self.redis.operation_timeout_ms > MAX_REDIS_OPERATION_TIMEOUT_MS {
            return Err(ConfigError::new(
                "REDIS_OPERATION_TIMEOUT_MS",
                "must be less than or equal to 60000",
            ));
        }
        if self.redis.event_stream_key.trim().is_empty()
            || self.redis.event_stream_key.chars().any(char::is_whitespace)
        {
            return Err(ConfigError::new(
                "REDIS_EVENT_STREAM_KEY",
                "must be a non-empty Redis key without whitespace",
            ));
        }
        if self.redis.event_stream_max_len == 0 {
            return Err(ConfigError::new(
                "REDIS_EVENT_STREAM_MAX_LEN",
                "must be greater than zero",
            ));
        }
        if self.redis.event_stream_batch_size == 0 {
            return Err(ConfigError::new(
                "REDIS_EVENT_STREAM_BATCH_SIZE",
                "must be greater than zero",
            ));
        }
        if self.redis.event_stream_batch_size > 1_000 {
            return Err(ConfigError::new(
                "REDIS_EVENT_STREAM_BATCH_SIZE",
                "must be less than or equal to 1000",
            ));
        }
        if self.redis.event_stream_interval_seconds == 0 {
            return Err(ConfigError::new(
                "REDIS_EVENT_STREAM_INTERVAL_SECONDS",
                "must be greater than zero",
            ));
        }
        if self.redis.event_stream_lease_seconds == 0 {
            return Err(ConfigError::new(
                "REDIS_EVENT_STREAM_LEASE_SECONDS",
                "must be greater than zero",
            ));
        }
        if self.redis.event_stream_retry_base_seconds == 0 {
            return Err(ConfigError::new(
                "REDIS_EVENT_STREAM_RETRY_BASE_SECONDS",
                "must be greater than zero",
            ));
        }
        if self.redis.event_stream_retry_max_seconds == 0 {
            return Err(ConfigError::new(
                "REDIS_EVENT_STREAM_RETRY_MAX_SECONDS",
                "must be greater than zero",
            ));
        }
        if self.redis.event_stream_retry_base_seconds > self.redis.event_stream_retry_max_seconds {
            return Err(ConfigError::new(
                "REDIS_EVENT_STREAM_RETRY_BASE_SECONDS",
                "must be less than or equal to REDIS_EVENT_STREAM_RETRY_MAX_SECONDS",
            ));
        }

        if self.metadata.providers.is_empty() {
            return Err(ConfigError::new(
                "METADATA_PROVIDERS",
                "must include at least one provider",
            ));
        }
        for provider in &self.metadata.providers {
            validate_metadata_provider(provider)?;
        }
        require_url("TMDB_API_BASE_URL", &self.metadata.tmdb_api_base_url)?;
        require_url("TMDB_IMAGE_BASE_URL", &self.metadata.tmdb_image_base_url)?;
        require_url("TVDB_API_BASE_URL", &self.metadata.tvdb_api_base_url)?;
        require_url("FANART_API_BASE_URL", &self.metadata.fanart_api_base_url)?;

        if self.media.roots.is_empty() {
            return Err(ConfigError::new(
                "MEDIA_ROOTS",
                "must include at least one media path",
            ));
        }

        if self.transcode.max_concurrent == 0 {
            return Err(ConfigError::new(
                "TRANSCODE_MAX_CONCURRENT",
                "must be greater than zero",
            ));
        }
        if self.transcode.lease_seconds == 0 {
            return Err(ConfigError::new(
                "TRANSCODE_LEASE_SECONDS",
                "must be greater than zero",
            ));
        }

        if let Some(secret_key) = &self.secrets.key {
            if secret_key.len() < 32 {
                return Err(ConfigError::new(
                    "FBZ_SECRET_KEY",
                    "must be at least 32 characters when configured",
                ));
            }
        }

        if self.plugins.timeout_ms == 0 {
            return Err(ConfigError::new(
                "PLUGIN_TIMEOUT_MS",
                "must be greater than zero",
            ));
        }

        if self.plugins.tmp_max_age_seconds == 0 {
            return Err(ConfigError::new(
                "PLUGIN_TMP_MAX_AGE_SECONDS",
                "must be greater than zero",
            ));
        }

        if self.plugins.max_concurrency == 0 {
            return Err(ConfigError::new(
                "PLUGIN_MAX_CONCURRENCY",
                "must be greater than zero",
            ));
        }

        if self.plugins.memory_limit_mb == 0 {
            return Err(ConfigError::new(
                "PLUGIN_MEMORY_LIMIT_MB",
                "must be greater than zero",
            ));
        }

        if self.plugins.wasi_fuel == 0 {
            return Err(ConfigError::new(
                "PLUGIN_WASI_FUEL",
                "must be greater than zero",
            ));
        }

        if self.plugins.wasi_stdio_max_bytes == 0 {
            return Err(ConfigError::new(
                "PLUGIN_WASI_STDIO_MAX_BYTES",
                "must be greater than zero",
            ));
        }
        if self.plugins.wasi_stdio_max_bytes > MAX_PLUGIN_WASI_STDIO_MAX_BYTES {
            return Err(ConfigError::new(
                "PLUGIN_WASI_STDIO_MAX_BYTES",
                format!("must be less than or equal to {MAX_PLUGIN_WASI_STDIO_MAX_BYTES}"),
            ));
        }

        if self.plugins.wasi_max_module_bytes == 0 {
            return Err(ConfigError::new(
                "PLUGIN_WASI_MAX_MODULE_BYTES",
                "must be greater than zero",
            ));
        }
        if self.plugins.wasi_max_module_bytes > MAX_PLUGIN_WASI_MAX_MODULE_BYTES {
            return Err(ConfigError::new(
                "PLUGIN_WASI_MAX_MODULE_BYTES",
                format!("must be less than or equal to {MAX_PLUGIN_WASI_MAX_MODULE_BYTES}"),
            ));
        }

        if self.plugins.http_max_response_body_bytes == 0 {
            return Err(ConfigError::new(
                "PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES",
                "must be greater than zero",
            ));
        }
        if self.plugins.http_max_response_body_bytes > MAX_PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES {
            return Err(ConfigError::new(
                "PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES",
                format!("must be less than or equal to {MAX_PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES}"),
            ));
        }

        if self.plugins.host_api_max_calls_per_run == 0 {
            return Err(ConfigError::new(
                "PLUGIN_HOST_API_MAX_CALLS_PER_RUN",
                "must be greater than zero",
            ));
        }
        if self.plugins.host_api_max_calls_per_run > MAX_PLUGIN_HOST_API_MAX_CALLS_PER_RUN {
            return Err(ConfigError::new(
                "PLUGIN_HOST_API_MAX_CALLS_PER_RUN",
                format!("must be less than or equal to {MAX_PLUGIN_HOST_API_MAX_CALLS_PER_RUN}"),
            ));
        }

        if let Some(secret_key) = &self.plugins.secret_key {
            if secret_key.len() < 32 {
                return Err(ConfigError::new(
                    "PLUGIN_SECRET_KEY",
                    "must be at least 32 characters when configured",
                ));
            }
        }

        for host in &self.plugins.http_allowed_hosts {
            validate_plugin_http_allowed_host(host)?;
        }

        if self.scan_worker.interval_seconds == 0 {
            return Err(ConfigError::new(
                "FBZ_SCAN_WORKER_INTERVAL_SECONDS",
                "must be greater than zero",
            ));
        }

        if self.scheduler.interval_seconds == 0 {
            return Err(ConfigError::new(
                "FBZ_SCHEDULER_INTERVAL_SECONDS",
                "must be greater than zero",
            ));
        }

        if self.transcode_worker.interval_seconds == 0 {
            return Err(ConfigError::new(
                "FBZ_TRANSCODE_WORKER_INTERVAL_SECONDS",
                "must be greater than zero",
            ));
        }

        if self.probe_worker.interval_seconds == 0 {
            return Err(ConfigError::new(
                "FBZ_PROBE_WORKER_INTERVAL_SECONDS",
                "must be greater than zero",
            ));
        }

        if self.photo_worker.interval_seconds == 0 {
            return Err(ConfigError::new(
                "FBZ_PHOTO_WORKER_INTERVAL_SECONDS",
                "must be greater than zero",
            ));
        }

        if self.metadata_worker.interval_seconds == 0 {
            return Err(ConfigError::new(
                "FBZ_METADATA_WORKER_INTERVAL_SECONDS",
                "must be greater than zero",
            ));
        }

        if self.plugin_worker.interval_seconds == 0 {
            return Err(ConfigError::new(
                "FBZ_PLUGIN_WORKER_INTERVAL_SECONDS",
                "must be greater than zero",
            ));
        }

        if self.notification_worker.interval_seconds == 0 {
            return Err(ConfigError::new(
                "FBZ_NOTIFICATION_WORKER_INTERVAL_SECONDS",
                "must be greater than zero",
            ));
        }

        if self.notification_worker.delivery_timeout_ms == 0 {
            return Err(ConfigError::new(
                "FBZ_NOTIFICATION_DELIVERY_TIMEOUT_MS",
                "must be greater than zero",
            ));
        }

        if self.schedules.incremental_scan.trim().is_empty() {
            return Err(ConfigError::new(
                "SCHEDULE_INCREMENTAL_SCAN",
                "must not be empty",
            ));
        }
        if self.schedules.metadata_refresh.trim().is_empty() {
            return Err(ConfigError::new(
                "SCHEDULE_METADATA_REFRESH",
                "must not be empty",
            ));
        }

        match (
            self.bootstrap_admin.username.as_deref(),
            self.bootstrap_admin.password.as_deref(),
        ) {
            (Some(username), Some(password)) => {
                if username.trim().is_empty() {
                    return Err(ConfigError::new(
                        "FBZ_BOOTSTRAP_ADMIN_USERNAME",
                        "must not be empty when configured",
                    ));
                }

                if !admin_password_meets_policy(password) {
                    return Err(ConfigError::new(
                        "FBZ_BOOTSTRAP_ADMIN_PASSWORD",
                        "must be at least 6 characters",
                    ));
                }
            }
            (None, None) => {}
            (Some(_), None) => {
                return Err(ConfigError::new(
                    "FBZ_BOOTSTRAP_ADMIN_PASSWORD",
                    "is required when FBZ_BOOTSTRAP_ADMIN_USERNAME is set",
                ));
            }
            (None, Some(_)) => {
                return Err(ConfigError::new(
                    "FBZ_BOOTSTRAP_ADMIN_USERNAME",
                    "is required when FBZ_BOOTSTRAP_ADMIN_PASSWORD is set",
                ));
            }
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_source(|_| None).expect("default config should be valid")
    }
}

fn get_or(key: &'static str, default: &str, source: &impl Fn(&str) -> Option<String>) -> String {
    get_or_else(key, source, || default.to_owned())
}

fn get_or_else(
    key: &'static str,
    source: &impl Fn(&str) -> Option<String>,
    default: impl FnOnce() -> String,
) -> String {
    optional(key, source).unwrap_or_else(default)
}

fn optional(key: &'static str, source: &impl Fn(&str) -> Option<String>) -> Option<String> {
    source(key).and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

fn parse_or<T>(
    key: &'static str,
    default: T,
    source: &impl Fn(&str) -> Option<String>,
) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: Display,
{
    match optional(key, source) {
        Some(value) => value
            .parse::<T>()
            .map_err(|err| ConfigError::new(key, err.to_string())),
        None => Ok(default),
    }
}

fn bool_or(
    key: &'static str,
    default: bool,
    source: &impl Fn(&str) -> Option<String>,
) -> Result<bool, ConfigError> {
    match optional(key, source) {
        Some(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(ConfigError::new(
                key,
                "expected boolean value true/false, yes/no, on/off, or 1/0",
            )),
        },
        None => Ok(default),
    }
}

fn csv_or(
    key: &'static str,
    default: &str,
    source: &impl Fn(&str) -> Option<String>,
) -> Vec<String> {
    let raw = get_or(key, default, source);
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn path_or(key: &'static str, default: &str, source: &impl Fn(&str) -> Option<String>) -> PathBuf {
    PathBuf::from(get_or(key, default, source))
}

fn paths_or(
    key: &'static str,
    default: &str,
    source: &impl Fn(&str) -> Option<String>,
) -> Vec<PathBuf> {
    csv_or(key, default, source)
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

fn plugin_trusted_signature_keys_or(
    key: &'static str,
    source: &impl Fn(&str) -> Option<String>,
) -> Result<Vec<PluginTrustedSignatureKey>, ConfigError> {
    let Some(raw) = optional(key, source) else {
        return Ok(Vec::new());
    };
    let mut keys = Vec::new();
    for entry in raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let Some((key_id, public_key_hex)) = entry.split_once(':') else {
            return Err(ConfigError::new(key, "entries must use keyId:publicKeyHex"));
        };
        validate_plugin_signature_key_id(key, key_id)?;
        if keys
            .iter()
            .any(|existing: &PluginTrustedSignatureKey| existing.key_id == key_id.trim())
        {
            return Err(ConfigError::new(key, "key ids must be unique"));
        }
        let public_key = parse_fixed_hex_32(key, public_key_hex.trim())?;
        keys.push(PluginTrustedSignatureKey {
            key_id: key_id.trim().to_owned(),
            public_key,
        });
    }
    Ok(keys)
}

fn validate_plugin_signature_key_id(
    config_key: &'static str,
    value: &str,
) -> Result<(), ConfigError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 64 {
        return Err(ConfigError::new(
            config_key,
            "key id must be 1 to 64 characters",
        ));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(ConfigError::new(
            config_key,
            "key id must contain only ascii letters, digits, dot, underscore, or dash",
        ));
    }
    Ok(())
}

fn parse_fixed_hex_32(key: &'static str, value: &str) -> Result<[u8; 32], ConfigError> {
    if value.len() != 64 {
        return Err(ConfigError::new(
            key,
            "public key hex must be 64 characters",
        ));
    }
    let mut bytes = [0_u8; 32];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(chunk[0])
            .ok_or_else(|| ConfigError::new(key, "public key hex is invalid"))?;
        let low = hex_nibble(chunk[1])
            .ok_or_else(|| ConfigError::new(key, "public key hex is invalid"))?;
        bytes[index] = high << 4 | low;
    }
    Ok(bytes)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn require_url(key: &'static str, value: &str) -> Result<(), ConfigError> {
    if value.starts_with("http://") || value.starts_with("https://") {
        return Ok(());
    }

    Err(ConfigError::new(key, "must start with http:// or https://"))
}

fn validate_plugin_http_allowed_host(value: &str) -> Result<(), ConfigError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ConfigError::new(
            "PLUGIN_HTTP_ALLOWED_HOSTS",
            "host entry must not be empty",
        ));
    }
    if value.contains(char::is_whitespace)
        || value.contains('/')
        || value.contains('\\')
        || value.contains(':') && value != "::1"
        || value.starts_with('.')
        || value.ends_with('.')
    {
        return Err(ConfigError::new(
            "PLUGIN_HTTP_ALLOWED_HOSTS",
            "entries must be bare hosts or wildcard suffixes like *.example.test",
        ));
    }
    if let Some(suffix) = value.strip_prefix("*.") {
        if suffix.is_empty() || suffix.starts_with('.') {
            return Err(ConfigError::new(
                "PLUGIN_HTTP_ALLOWED_HOSTS",
                "wildcard suffix must include a domain",
            ));
        }
    }
    Ok(())
}

fn validate_metadata_provider(provider: &str) -> Result<(), ConfigError> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "tmdb" | "tvdb" | "fanart" => Ok(()),
        other => Err(ConfigError::new(
            "METADATA_PROVIDERS",
            format!("unsupported metadata provider `{other}`"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn default_config_is_suitable_for_local_development() {
        let config = Config::default();

        assert_eq!(config.server.host, DEFAULT_HOST);
        assert_eq!(config.server.port, DEFAULT_PORT);
        assert_eq!(
            config.server.readiness_timeout_ms,
            DEFAULT_READINESS_TIMEOUT_MS
        );
        assert_eq!(
            config.telemetry.http_slow_log_threshold_ms,
            DEFAULT_HTTP_SLOW_LOG_THRESHOLD_MS
        );
        assert_eq!(config.database.url, DEFAULT_DATABASE_URL);
        assert_eq!(config.database.min_connections, 1);
        assert_eq!(config.database.max_connections, 20);
        assert_eq!(
            config.database.acquire_timeout_seconds,
            DEFAULT_DATABASE_ACQUIRE_TIMEOUT_SECONDS
        );
        assert_eq!(
            config.database.idle_timeout_seconds,
            DEFAULT_DATABASE_IDLE_TIMEOUT_SECONDS
        );
        assert_eq!(
            config.database.max_lifetime_seconds,
            DEFAULT_DATABASE_MAX_LIFETIME_SECONDS
        );
        assert_eq!(
            config.database.statement_timeout_ms,
            DEFAULT_DATABASE_STATEMENT_TIMEOUT_MS
        );
        assert_eq!(
            config.database.slow_log_threshold_ms,
            DEFAULT_DATABASE_SLOW_LOG_THRESHOLD_MS
        );
        assert_eq!(config.redis.url, DEFAULT_REDIS_URL);
        assert_eq!(
            config.redis.operation_timeout_ms,
            DEFAULT_REDIS_OPERATION_TIMEOUT_MS
        );
        assert!(!config.redis.event_streams_enabled);
        assert_eq!(
            config.redis.event_stream_key,
            DEFAULT_REDIS_EVENT_STREAM_KEY
        );
        assert_eq!(
            config.redis.event_stream_max_len,
            DEFAULT_REDIS_EVENT_STREAM_MAX_LEN
        );
        assert_eq!(
            config.redis.event_stream_batch_size,
            DEFAULT_REDIS_EVENT_STREAM_BATCH_SIZE
        );
        assert_eq!(
            config.redis.event_stream_interval_seconds,
            DEFAULT_REDIS_EVENT_STREAM_INTERVAL_SECONDS
        );
        assert_eq!(
            config.redis.event_stream_lease_seconds,
            DEFAULT_REDIS_EVENT_STREAM_LEASE_SECONDS
        );
        assert_eq!(
            config.redis.event_stream_retry_base_seconds,
            DEFAULT_REDIS_EVENT_STREAM_RETRY_BASE_SECONDS
        );
        assert_eq!(
            config.redis.event_stream_retry_max_seconds,
            DEFAULT_REDIS_EVENT_STREAM_RETRY_MAX_SECONDS
        );
        assert_eq!(config.secrets.key, None);
        assert_eq!(config.media_tools.ffmpeg_path, "ffmpeg");
        assert_eq!(config.media_tools.ffprobe_path, "ffprobe");
        assert_eq!(config.transcode.max_concurrent, 3);
        assert_eq!(config.transcode.lease_seconds, 900);
        assert_eq!(config.plugins.runtime_default, "wasi");
        assert!(config.plugins.require_approval);
        assert!(!config.plugins.allow_unsigned);
        assert!(config.plugins.trusted_signature_keys.is_empty());
        assert_eq!(
            config.plugins.tmp_max_age_seconds,
            DEFAULT_PLUGIN_TMP_MAX_AGE_SECONDS
        );
        assert_eq!(config.plugins.wasi_fuel, DEFAULT_PLUGIN_WASI_FUEL);
        assert_eq!(
            config.plugins.wasi_stdio_max_bytes,
            DEFAULT_PLUGIN_WASI_STDIO_MAX_BYTES
        );
        assert_eq!(
            config.plugins.wasi_max_module_bytes,
            DEFAULT_PLUGIN_WASI_MAX_MODULE_BYTES
        );
        assert_eq!(
            config.plugins.http_max_response_body_bytes,
            DEFAULT_PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES
        );
        assert_eq!(
            config.plugins.host_api_max_calls_per_run,
            DEFAULT_PLUGIN_HOST_API_MAX_CALLS_PER_RUN
        );
        assert_eq!(
            config.plugins.http_allowed_hosts,
            ["127.0.0.1", "localhost", "::1", "host.docker.internal"]
        );
        assert!(!config.scan_worker.enabled);
        assert_eq!(config.scan_worker.interval_seconds, 5);
        assert!(!config.scheduler.enabled);
        assert_eq!(config.scheduler.interval_seconds, 5);
        assert!(!config.transcode_worker.enabled);
        assert_eq!(config.transcode_worker.interval_seconds, 5);
        assert!(!config.probe_worker.enabled);
        assert_eq!(config.probe_worker.interval_seconds, 10);
        assert!(!config.metadata_worker.enabled);
        assert_eq!(config.metadata_worker.interval_seconds, 10);
        assert!(!config.plugin_worker.enabled);
        assert_eq!(config.plugin_worker.interval_seconds, 5);
        assert!(!config.notification_worker.enabled);
        assert_eq!(config.notification_worker.interval_seconds, 5);
        assert_eq!(config.notification_worker.delivery_timeout_ms, 5_000);
        assert_eq!(config.bootstrap_admin.username, None);
    }

    #[test]
    fn config_supports_environment_overrides() {
        let source = map_source([
            ("FBZ_API_HOST", "0.0.0.0"),
            ("FBZ_API_PORT", "8096"),
            ("FBZ_READINESS_TIMEOUT_MS", "750"),
            ("HTTP_SLOW_LOG_THRESHOLD_MS", "2500"),
            ("PUBLIC_BASE_URL", "https://media.example.test"),
            ("DATABASE_URL", "postgresql://fbz:secret@db/fbz"),
            ("DATABASE_MIN_CONNECTIONS", "2"),
            ("DATABASE_MAX_CONNECTIONS", "8"),
            ("DATABASE_ACQUIRE_TIMEOUT_SECONDS", "3"),
            ("DATABASE_IDLE_TIMEOUT_SECONDS", "300"),
            ("DATABASE_MAX_LIFETIME_SECONDS", "900"),
            ("DATABASE_STATEMENT_TIMEOUT_MS", "12000"),
            ("DATABASE_SLOW_LOG_THRESHOLD_MS", "450"),
            ("REDIS_URL", "rediss://redis.example.test:6380"),
            ("REDIS_OPERATION_TIMEOUT_MS", "1500"),
            ("REDIS_EVENT_STREAMS_ENABLED", "true"),
            ("REDIS_EVENT_STREAM_KEY", "fbz:test-events"),
            ("REDIS_EVENT_STREAM_MAX_LEN", "25000"),
            ("REDIS_EVENT_STREAM_BATCH_SIZE", "50"),
            ("REDIS_EVENT_STREAM_INTERVAL_SECONDS", "7"),
            ("REDIS_EVENT_STREAM_LEASE_SECONDS", "45"),
            ("REDIS_EVENT_STREAM_RETRY_BASE_SECONDS", "4"),
            ("REDIS_EVENT_STREAM_RETRY_MAX_SECONDS", "64"),
            ("METADATA_PROVIDERS", "tmdb,fanart"),
            ("MEDIA_ROOTS", "/media/movies,/media/tv"),
            ("TRANSCODE_MAX_CONCURRENT", "2"),
            ("TRANSCODE_LEASE_SECONDS", "1200"),
            ("PLUGIN_REQUIRE_APPROVAL", "false"),
            ("PLUGIN_ALLOW_UNSIGNED", "true"),
            (
                "PLUGIN_TRUSTED_SIGNATURE_KEYS",
                "dev-key:000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
            ),
            (
                "PLUGIN_HTTP_ALLOWED_HOSTS",
                "plugins.internal,*.example.test",
            ),
            ("FBZ_SCAN_WORKER_ENABLED", "true"),
            ("PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES", "32768"),
            ("PLUGIN_TMP_MAX_AGE_SECONDS", "3600"),
            ("FBZ_SCAN_WORKER_INTERVAL_SECONDS", "2"),
            ("FBZ_SCHEDULER_ENABLED", "true"),
            ("FBZ_SCHEDULER_INTERVAL_SECONDS", "3"),
            ("FBZ_TRANSCODE_WORKER_ENABLED", "true"),
            ("FBZ_TRANSCODE_WORKER_INTERVAL_SECONDS", "8"),
            ("FBZ_PROBE_WORKER_ENABLED", "true"),
            ("FBZ_PROBE_WORKER_INTERVAL_SECONDS", "11"),
            ("FBZ_METADATA_WORKER_ENABLED", "true"),
            ("FBZ_METADATA_WORKER_INTERVAL_SECONDS", "9"),
            ("FBZ_PLUGIN_WORKER_ENABLED", "true"),
            ("FBZ_PLUGIN_WORKER_INTERVAL_SECONDS", "4"),
            ("PLUGIN_WASI_FUEL", "123456"),
            ("PLUGIN_WASI_STDIO_MAX_BYTES", "4096"),
            ("PLUGIN_WASI_MAX_MODULE_BYTES", "1048576"),
            ("PLUGIN_HOST_API_MAX_CALLS_PER_RUN", "2500"),
            ("FBZ_NOTIFICATION_WORKER_ENABLED", "true"),
            ("FBZ_NOTIFICATION_WORKER_INTERVAL_SECONDS", "6"),
            ("FBZ_NOTIFICATION_DELIVERY_TIMEOUT_MS", "7000"),
            ("FBZ_SECRET_KEY", "0123456789abcdef0123456789abcdef"),
            ("PLUGIN_SECRET_KEY", "abcdef0123456789abcdef0123456789"),
        ]);

        let config = Config::from_source(|key| source.get(key).cloned()).unwrap();

        assert_eq!(config.server.host, "0.0.0.0".parse::<IpAddr>().unwrap());
        assert_eq!(config.server.port, 8096);
        assert_eq!(config.server.readiness_timeout_ms, 750);
        assert_eq!(config.telemetry.http_slow_log_threshold_ms, 2_500);
        assert_eq!(config.server.public_base_url, "https://media.example.test");
        assert_eq!(config.database.url, "postgresql://fbz:secret@db/fbz");
        assert_eq!(config.database.min_connections, 2);
        assert_eq!(config.database.max_connections, 8);
        assert_eq!(config.database.acquire_timeout_seconds, 3);
        assert_eq!(config.database.idle_timeout_seconds, 300);
        assert_eq!(config.database.max_lifetime_seconds, 900);
        assert_eq!(config.database.statement_timeout_ms, 12_000);
        assert_eq!(config.database.slow_log_threshold_ms, 450);
        assert_eq!(config.redis.url, "rediss://redis.example.test:6380");
        assert_eq!(config.redis.operation_timeout_ms, 1_500);
        assert!(config.redis.event_streams_enabled);
        assert_eq!(config.redis.event_stream_key, "fbz:test-events");
        assert_eq!(config.redis.event_stream_max_len, 25_000);
        assert_eq!(config.redis.event_stream_batch_size, 50);
        assert_eq!(config.redis.event_stream_interval_seconds, 7);
        assert_eq!(config.redis.event_stream_lease_seconds, 45);
        assert_eq!(config.redis.event_stream_retry_base_seconds, 4);
        assert_eq!(config.redis.event_stream_retry_max_seconds, 64);
        assert_eq!(config.metadata.providers, ["tmdb", "fanart"]);
        assert_eq!(
            config.media.roots,
            [PathBuf::from("/media/movies"), PathBuf::from("/media/tv")]
        );
        assert_eq!(config.transcode.max_concurrent, 2);
        assert_eq!(config.transcode.lease_seconds, 1_200);
        assert!(!config.plugins.require_approval);
        assert!(config.plugins.allow_unsigned);
        assert_eq!(config.plugins.trusted_signature_keys.len(), 1);
        assert_eq!(config.plugins.trusted_signature_keys[0].key_id, "dev-key");
        assert_eq!(
            config.plugins.trusted_signature_keys[0].public_key,
            [
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
                0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
                0x1c, 0x1d, 0x1e, 0x1f,
            ]
        );
        assert_eq!(
            config.plugins.http_allowed_hosts,
            ["plugins.internal", "*.example.test"]
        );
        assert_eq!(config.plugins.http_max_response_body_bytes, 32_768);
        assert_eq!(config.plugins.tmp_max_age_seconds, 3_600);
        assert_eq!(config.plugins.wasi_fuel, 123_456);
        assert_eq!(config.plugins.wasi_stdio_max_bytes, 4_096);
        assert_eq!(config.plugins.wasi_max_module_bytes, 1_048_576);
        assert_eq!(config.plugins.host_api_max_calls_per_run, 2_500);
        assert!(config.scan_worker.enabled);
        assert_eq!(config.scan_worker.interval_seconds, 2);
        assert!(config.scheduler.enabled);
        assert_eq!(config.scheduler.interval_seconds, 3);
        assert!(config.transcode_worker.enabled);
        assert_eq!(config.transcode_worker.interval_seconds, 8);
        assert!(config.probe_worker.enabled);
        assert_eq!(config.probe_worker.interval_seconds, 11);
        assert!(config.metadata_worker.enabled);
        assert_eq!(config.metadata_worker.interval_seconds, 9);
        assert!(config.plugin_worker.enabled);
        assert_eq!(config.plugin_worker.interval_seconds, 4);
        assert!(config.notification_worker.enabled);
        assert_eq!(config.notification_worker.interval_seconds, 6);
        assert_eq!(config.notification_worker.delivery_timeout_ms, 7_000);
        assert_eq!(
            config.secrets.key.as_deref(),
            Some("0123456789abcdef0123456789abcdef")
        );
        assert_eq!(
            config.plugins.secret_key.as_deref(),
            Some("abcdef0123456789abcdef0123456789")
        );
    }

    #[test]
    fn invalid_node_role_fails_early() {
        let source = map_source([("FBZ_NODE_ROLE", "database")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "FBZ_NODE_ROLE");
        assert!(err.to_string().contains("unsupported node role"));
    }

    #[test]
    fn invalid_readiness_timeout_fails_early() {
        let source = map_source([("FBZ_READINESS_TIMEOUT_MS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "FBZ_READINESS_TIMEOUT_MS");

        let source = map_source([("FBZ_READINESS_TIMEOUT_MS", "60001")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "FBZ_READINESS_TIMEOUT_MS");
        assert!(err.to_string().contains("less than or equal to 60000"));
    }

    #[test]
    fn invalid_http_slow_log_threshold_fails_early() {
        let source = map_source([("HTTP_SLOW_LOG_THRESHOLD_MS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "HTTP_SLOW_LOG_THRESHOLD_MS");
    }

    #[test]
    fn invalid_database_url_fails_early() {
        let source = map_source([("DATABASE_URL", "mysql://fbz:secret@db/fbz")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "DATABASE_URL");
    }

    #[test]
    fn invalid_database_pool_config_fails_early() {
        let source = map_source([
            ("DATABASE_MIN_CONNECTIONS", "8"),
            ("DATABASE_MAX_CONNECTIONS", "2"),
        ]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "DATABASE_MIN_CONNECTIONS");

        let source = map_source([("DATABASE_MAX_CONNECTIONS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "DATABASE_MAX_CONNECTIONS");

        let source = map_source([("DATABASE_ACQUIRE_TIMEOUT_SECONDS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "DATABASE_ACQUIRE_TIMEOUT_SECONDS");

        let source = map_source([("DATABASE_IDLE_TIMEOUT_SECONDS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "DATABASE_IDLE_TIMEOUT_SECONDS");

        let source = map_source([("DATABASE_MAX_LIFETIME_SECONDS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "DATABASE_MAX_LIFETIME_SECONDS");

        let source = map_source([("DATABASE_STATEMENT_TIMEOUT_MS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "DATABASE_STATEMENT_TIMEOUT_MS");

        let source = map_source([("DATABASE_SLOW_LOG_THRESHOLD_MS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "DATABASE_SLOW_LOG_THRESHOLD_MS");
    }

    #[test]
    fn invalid_secret_key_fails_early() {
        let source = map_source([("FBZ_SECRET_KEY", "short")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "FBZ_SECRET_KEY");
    }

    #[test]
    fn invalid_plugin_limit_fails_early() {
        let source = map_source([("PLUGIN_MAX_CONCURRENCY", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_MAX_CONCURRENCY");

        let source = map_source([("PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES");

        let source = map_source([("PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES", "1048577")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES");

        let source = map_source([("PLUGIN_HOST_API_MAX_CALLS_PER_RUN", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_HOST_API_MAX_CALLS_PER_RUN");

        let source = map_source([("PLUGIN_HOST_API_MAX_CALLS_PER_RUN", "100001")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_HOST_API_MAX_CALLS_PER_RUN");

        let source = map_source([("PLUGIN_TMP_MAX_AGE_SECONDS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_TMP_MAX_AGE_SECONDS");

        let source = map_source([("PLUGIN_WASI_FUEL", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_WASI_FUEL");

        let source = map_source([("PLUGIN_WASI_STDIO_MAX_BYTES", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_WASI_STDIO_MAX_BYTES");

        let source = map_source([("PLUGIN_WASI_STDIO_MAX_BYTES", "1048577")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_WASI_STDIO_MAX_BYTES");

        let source = map_source([("PLUGIN_WASI_MAX_MODULE_BYTES", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_WASI_MAX_MODULE_BYTES");

        let source = map_source([("PLUGIN_WASI_MAX_MODULE_BYTES", "268435457")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_WASI_MAX_MODULE_BYTES");
    }

    #[test]
    fn invalid_redis_event_stream_config_fails_early() {
        let source = map_source([("REDIS_OPERATION_TIMEOUT_MS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "REDIS_OPERATION_TIMEOUT_MS");

        let source = map_source([("REDIS_OPERATION_TIMEOUT_MS", "60001")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "REDIS_OPERATION_TIMEOUT_MS");

        let source = map_source([("REDIS_EVENT_STREAM_KEY", "fbz events")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "REDIS_EVENT_STREAM_KEY");

        let source = map_source([("REDIS_EVENT_STREAM_BATCH_SIZE", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "REDIS_EVENT_STREAM_BATCH_SIZE");

        let source = map_source([("REDIS_EVENT_STREAM_MAX_LEN", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "REDIS_EVENT_STREAM_MAX_LEN");

        let source = map_source([("REDIS_EVENT_STREAM_INTERVAL_SECONDS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "REDIS_EVENT_STREAM_INTERVAL_SECONDS");

        let source = map_source([("REDIS_EVENT_STREAM_LEASE_SECONDS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "REDIS_EVENT_STREAM_LEASE_SECONDS");

        let source = map_source([("REDIS_EVENT_STREAM_RETRY_BASE_SECONDS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "REDIS_EVENT_STREAM_RETRY_BASE_SECONDS");

        let source = map_source([("REDIS_EVENT_STREAM_RETRY_MAX_SECONDS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "REDIS_EVENT_STREAM_RETRY_MAX_SECONDS");

        let source = map_source([
            ("REDIS_EVENT_STREAM_RETRY_BASE_SECONDS", "30"),
            ("REDIS_EVENT_STREAM_RETRY_MAX_SECONDS", "10"),
        ]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "REDIS_EVENT_STREAM_RETRY_BASE_SECONDS");
    }

    #[test]
    fn invalid_plugin_http_allowed_host_fails_early() {
        let source = map_source([("PLUGIN_HTTP_ALLOWED_HOSTS", "https://plugin.example.test")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_HTTP_ALLOWED_HOSTS");
    }

    #[test]
    fn invalid_plugin_trusted_signature_keys_fail_early() {
        let source = map_source([("PLUGIN_TRUSTED_SIGNATURE_KEYS", "missing-colon")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_TRUSTED_SIGNATURE_KEYS");

        let source = map_source([("PLUGIN_TRUSTED_SIGNATURE_KEYS", "bad key:00")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_TRUSTED_SIGNATURE_KEYS");

        let source = map_source([(
            "PLUGIN_TRUSTED_SIGNATURE_KEYS",
            "dev:000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f,dev:101112131415161718191a1b1c1d1e1f000102030405060708090a0b0c0d0e0f",
        )]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_TRUSTED_SIGNATURE_KEYS");
    }

    #[test]
    fn invalid_plugin_secret_key_fails_early() {
        let source = map_source([("PLUGIN_SECRET_KEY", "short")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "PLUGIN_SECRET_KEY");
    }

    #[test]
    fn invalid_scan_worker_interval_fails_early() {
        let source = map_source([("FBZ_SCAN_WORKER_INTERVAL_SECONDS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "FBZ_SCAN_WORKER_INTERVAL_SECONDS");
    }

    #[test]
    fn invalid_scheduler_interval_fails_early() {
        let source = map_source([("FBZ_SCHEDULER_INTERVAL_SECONDS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "FBZ_SCHEDULER_INTERVAL_SECONDS");
    }

    #[test]
    fn invalid_plugin_worker_interval_fails_early() {
        let source = map_source([("FBZ_PLUGIN_WORKER_INTERVAL_SECONDS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "FBZ_PLUGIN_WORKER_INTERVAL_SECONDS");
    }

    #[test]
    fn invalid_probe_worker_interval_fails_early() {
        let source = map_source([("FBZ_PROBE_WORKER_INTERVAL_SECONDS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "FBZ_PROBE_WORKER_INTERVAL_SECONDS");
    }

    #[test]
    fn invalid_metadata_provider_fails_early() {
        let source = map_source([("METADATA_PROVIDERS", "tmdb,unknown")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "METADATA_PROVIDERS");
    }

    #[test]
    fn invalid_metadata_worker_interval_fails_early() {
        let source = map_source([("FBZ_METADATA_WORKER_INTERVAL_SECONDS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "FBZ_METADATA_WORKER_INTERVAL_SECONDS");
    }

    #[test]
    fn invalid_notification_worker_config_fails_early() {
        let source = map_source([("FBZ_NOTIFICATION_WORKER_INTERVAL_SECONDS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "FBZ_NOTIFICATION_WORKER_INTERVAL_SECONDS");

        let source = map_source([("FBZ_NOTIFICATION_DELIVERY_TIMEOUT_MS", "0")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "FBZ_NOTIFICATION_DELIVERY_TIMEOUT_MS");
    }

    #[test]
    fn bootstrap_admin_requires_username_and_password() {
        let source = map_source([("FBZ_BOOTSTRAP_ADMIN_USERNAME", "admin")]);

        let err = Config::from_source(|key| source.get(key).cloned()).unwrap_err();

        assert_eq!(err.key(), "FBZ_BOOTSTRAP_ADMIN_PASSWORD");
    }

    fn map_source<const N: usize>(entries: [(&str, &str); N]) -> HashMap<String, String> {
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect()
    }
}
