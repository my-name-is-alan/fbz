use std::{
    error::Error,
    fmt::{Display, Formatter},
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use serde_json::Value;
use tokio::task;
use tracing::warn;
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, Trap};
use wasmtime_wasi::{
    DirPerms, FilePerms, I32Exit, WasiCtxBuilder,
    p1::{self, WasiP1Ctx},
    p2::pipe::{MemoryInputPipe, MemoryOutputPipe},
};

const PLUGIN_PACKAGE_EXTRACTED_DIR: &str = "extracted";
const GUEST_PACKAGE_DIR: &str = "/plugin";
const GUEST_DATA_DIR: &str = "/data";
const GUEST_CACHE_DIR: &str = "/cache";
const GUEST_TMP_DIR: &str = "/tmp";
const MAX_WASI_AUDIT_BYTES: usize = 4096;
const WASI_EPOCH_TICK_MILLIS: u64 = 10;

#[derive(Clone)]
pub struct PluginWasiRuntime {
    engine: Arc<Engine>,
    epoch_ticker: Arc<PluginWasiEpochTicker>,
}

#[derive(Debug)]
pub struct PluginWasiExecution {
    pub package_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub plugin_id: String,
    pub package_version: String,
    pub entrypoint: String,
    pub handler: String,
    pub idempotency_key: String,
    pub host_base_url: String,
    pub host_token: Option<String>,
    pub payload: Value,
    pub timeout: Duration,
    pub memory_limit_mb: u16,
    pub fuel: u64,
    pub stdio_max_bytes: usize,
    pub max_module_bytes: u64,
    pub tmp_max_age: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginWasiOutput {
    pub response_body: Option<String>,
}

#[derive(Debug)]
pub enum PluginWasiError {
    Entrypoint(String),
    Io(String),
    Compile(String),
    Runtime(String),
    Timeout,
    Join(String),
    Exit {
        code: i32,
        stdout: String,
        stderr: String,
    },
}

struct PluginWasiState {
    wasi: WasiP1Ctx,
    limits: StoreLimits,
}

struct PluginWasiEpochTicker {
    active: AtomicUsize,
    stop: AtomicBool,
    handle: Mutex<Option<JoinHandle<()>>>,
}

struct PluginWasiEpochGuard {
    ticker: Arc<PluginWasiEpochTicker>,
}

impl PluginWasiRuntime {
    pub fn new() -> Self {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine =
            Arc::new(Engine::new(&config).expect("plugin WASI engine config should be valid"));
        let epoch_ticker = PluginWasiEpochTicker::start(engine.clone());

        Self {
            engine,
            epoch_ticker,
        }
    }

    pub async fn execute(
        &self,
        execution: PluginWasiExecution,
    ) -> Result<PluginWasiOutput, PluginWasiError> {
        let engine = self.engine.clone();
        let _epoch_guard = self.epoch_ticker.execution_guard();
        task::spawn_blocking(move || execute_wasi_blocking(&engine, execution))
            .await
            .map_err(|err| PluginWasiError::Join(err.to_string()))?
    }
}

impl PluginWasiEpochTicker {
    fn start(engine: Arc<Engine>) -> Arc<Self> {
        let ticker = Arc::new(Self {
            active: AtomicUsize::new(0),
            stop: AtomicBool::new(false),
            handle: Mutex::new(None),
        });
        let weak_ticker = Arc::downgrade(&ticker);
        let handle = thread::Builder::new()
            .name("fbz-wasi-epoch".to_owned())
            .spawn(move || {
                loop {
                    let Some(ticker) = weak_ticker.upgrade() else {
                        break;
                    };
                    if ticker.stop.load(Ordering::Acquire) {
                        break;
                    }
                    if ticker.active.load(Ordering::Acquire) > 0 {
                        engine.increment_epoch();
                    }
                    drop(ticker);
                    thread::sleep(Duration::from_millis(WASI_EPOCH_TICK_MILLIS));
                }
            })
            .expect("plugin WASI epoch ticker thread should start");

        *ticker
            .handle
            .lock()
            .expect("plugin WASI epoch ticker lock should be available") = Some(handle);
        ticker
    }

    fn execution_guard(self: &Arc<Self>) -> PluginWasiEpochGuard {
        self.active.fetch_add(1, Ordering::AcqRel);
        PluginWasiEpochGuard {
            ticker: self.clone(),
        }
    }
}

impl Drop for PluginWasiEpochGuard {
    fn drop(&mut self) {
        self.ticker.active.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Drop for PluginWasiEpochTicker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Ok(mut handle) = self.handle.lock()
            && let Some(handle) = handle.take()
        {
            let _ = handle.join();
        }
    }
}

fn execute_wasi_blocking(
    engine: &Engine,
    execution: PluginWasiExecution,
) -> Result<PluginWasiOutput, PluginWasiError> {
    let plugin_tmp_root = prepare_plugin_temp_dir(
        &execution.tmp_dir,
        &execution.plugin_id,
        &execution.idempotency_key,
        execution.tmp_max_age,
    )?;
    let result = execute_wasi_with_mounts(engine, execution, &plugin_tmp_root);
    cleanup_plugin_temp_dir(&plugin_tmp_root);
    result
}

fn execute_wasi_with_mounts(
    engine: &Engine,
    execution: PluginWasiExecution,
    plugin_tmp_root: &Path,
) -> Result<PluginWasiOutput, PluginWasiError> {
    let package_root = plugin_package_extract_dir(
        &execution.package_dir,
        &execution.plugin_id,
        &execution.package_version,
    );
    let plugin_data_root = prepare_plugin_writable_dir(&execution.data_dir, &execution.plugin_id)?;
    let plugin_cache_root =
        prepare_plugin_writable_dir(&execution.cache_dir, &execution.plugin_id)?;
    let entrypoint_path = resolve_wasi_entrypoint(&package_root, &execution.entrypoint)?;
    validate_wasi_module_file(&entrypoint_path, execution.max_module_bytes)?;

    let module = Module::from_file(engine, &entrypoint_path)
        .map_err(|err| PluginWasiError::Compile(err.to_string()))?;
    let mut linker = Linker::<PluginWasiState>::new(engine);
    p1::add_to_linker_sync(&mut linker, |state| &mut state.wasi)
        .map_err(|err| PluginWasiError::Runtime(err.to_string()))?;

    let payload_bytes = serde_json::to_vec(&execution.payload)
        .map_err(|err| PluginWasiError::Runtime(err.to_string()))?;
    let stdout = MemoryOutputPipe::new(execution.stdio_max_bytes);
    let stderr = MemoryOutputPipe::new(execution.stdio_max_bytes);
    let mut wasi = WasiCtxBuilder::new();
    wasi.arg(execution.entrypoint.trim())
        .arg(execution.handler.trim())
        .env("FBZ_PLUGIN_ID", execution.plugin_id.trim())
        .env("FBZ_PLUGIN_HANDLER", execution.handler.trim())
        .env(
            "FBZ_PLUGIN_IDEMPOTENCY_KEY",
            execution.idempotency_key.trim(),
        )
        .env("FBZ_HOST_BASE_URL", execution.host_base_url.trim())
        .stdin(MemoryInputPipe::new(Bytes::from(payload_bytes)))
        .stdout(stdout.clone())
        .stderr(stderr.clone())
        .preopened_dir(
            &package_root,
            GUEST_PACKAGE_DIR,
            DirPerms::READ,
            FilePerms::READ,
        )
        .map_err(|err| PluginWasiError::Runtime(err.to_string()))?
        .preopened_dir(
            &plugin_data_root,
            GUEST_DATA_DIR,
            DirPerms::READ | DirPerms::MUTATE,
            FilePerms::READ | FilePerms::WRITE,
        )
        .map_err(|err| PluginWasiError::Runtime(err.to_string()))?
        .preopened_dir(
            &plugin_cache_root,
            GUEST_CACHE_DIR,
            DirPerms::READ | DirPerms::MUTATE,
            FilePerms::READ | FilePerms::WRITE,
        )
        .map_err(|err| PluginWasiError::Runtime(err.to_string()))?
        .preopened_dir(
            plugin_tmp_root,
            GUEST_TMP_DIR,
            DirPerms::READ | DirPerms::MUTATE,
            FilePerms::READ | FilePerms::WRITE,
        )
        .map_err(|err| PluginWasiError::Runtime(err.to_string()))?;
    if let Some(host_token) = execution
        .host_token
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        wasi.env("FBZ_PLUGIN_TOKEN", host_token);
    }

    let memory_limit = usize::from(execution.memory_limit_mb) * 1024 * 1024;
    let limits = StoreLimitsBuilder::new()
        .memory_size(memory_limit)
        .instances(1)
        .memories(2)
        .tables(4)
        .trap_on_grow_failure(true)
        .build();
    let state = PluginWasiState {
        wasi: wasi.build_p1(),
        limits,
    };
    let mut store = Store::new(engine, state);
    store.limiter(|state| &mut state.limits);
    store.epoch_deadline_trap();
    store.set_epoch_deadline(epoch_deadline_ticks(execution.timeout));
    store
        .set_fuel(execution.fuel)
        .map_err(|err| PluginWasiError::Runtime(err.to_string()))?;

    let pre = linker
        .instantiate_pre(&module)
        .map_err(|err| PluginWasiError::Runtime(err.to_string()))?;
    let instance = pre
        .instantiate(&mut store)
        .map_err(|err| PluginWasiError::Runtime(err.to_string()))?;
    let start = instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .map_err(|err| PluginWasiError::Runtime(err.to_string()))?;

    match start.call(&mut store, ()) {
        Ok(()) => Ok(PluginWasiOutput {
            response_body: wasi_response_body(&captured_stdout(&stdout), &captured_stdout(&stderr)),
        }),
        Err(err) => {
            let stdout = captured_stdout(&stdout);
            let stderr = captured_stdout(&stderr);
            if let Some(exit) = err.downcast_ref::<I32Exit>() {
                if exit.0 == 0 {
                    return Ok(PluginWasiOutput {
                        response_body: wasi_response_body(&stdout, &stderr),
                    });
                }
                return Err(PluginWasiError::Exit {
                    code: exit.0,
                    stdout,
                    stderr,
                });
            }

            if is_epoch_interruption(&err) {
                return Err(PluginWasiError::Timeout);
            }

            Err(PluginWasiError::Runtime(format!(
                "{}{}",
                err,
                wasi_error_suffix(&stdout, &stderr)
            )))
        }
    }
}

pub fn plugin_package_extract_dir(
    package_dir: &Path,
    plugin_id: &str,
    package_version: &str,
) -> PathBuf {
    package_dir
        .join(PLUGIN_PACKAGE_EXTRACTED_DIR)
        .join(plugin_id.trim())
        .join(package_version.trim())
}

fn plugin_scoped_dir(root: &Path, plugin_id: &str) -> Result<PathBuf, PluginWasiError> {
    validate_plugin_dir_name(plugin_id)?;
    Ok(root.join(plugin_id.trim()))
}

fn prepare_plugin_writable_dir(root: &Path, plugin_id: &str) -> Result<PathBuf, PluginWasiError> {
    let path = plugin_scoped_dir(root, plugin_id)?;
    std::fs::create_dir_all(&path).map_err(|err| {
        PluginWasiError::Io(format!(
            "failed to create plugin scoped writable directory `{}`: {err}",
            path.display()
        ))
    })?;
    Ok(path)
}

fn prepare_plugin_temp_dir(
    root: &Path,
    plugin_id: &str,
    idempotency_key: &str,
    max_age: Duration,
) -> Result<PathBuf, PluginWasiError> {
    let plugin_root = plugin_scoped_dir(root, plugin_id)?;
    std::fs::create_dir_all(&plugin_root).map_err(|err| {
        PluginWasiError::Io(format!(
            "failed to create plugin scoped temp parent `{}`: {err}",
            plugin_root.display()
        ))
    })?;
    cleanup_stale_plugin_temp_dirs(&plugin_root, max_age);

    let temp_name = format!(
        "{}-{}",
        sanitize_dispatch_segment(idempotency_key)?,
        unix_timestamp_nanos()
    );
    let path = plugin_root.join(temp_name);
    std::fs::create_dir(&path).map_err(|err| {
        PluginWasiError::Io(format!(
            "failed to create plugin scoped temp directory `{}`: {err}",
            path.display()
        ))
    })?;
    Ok(path)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PluginTempScavengeSummary {
    removed_dirs: usize,
    kept_entries: usize,
    errors: usize,
}

fn cleanup_stale_plugin_temp_dirs(
    plugin_root: &Path,
    max_age: Duration,
) -> PluginTempScavengeSummary {
    cleanup_stale_plugin_temp_dirs_at(plugin_root, max_age, SystemTime::now())
}

fn cleanup_stale_plugin_temp_dirs_at(
    plugin_root: &Path,
    max_age: Duration,
    now: SystemTime,
) -> PluginTempScavengeSummary {
    let mut summary = PluginTempScavengeSummary::default();
    let entries = match std::fs::read_dir(plugin_root) {
        Ok(entries) => entries,
        Err(err) => {
            summary.errors += 1;
            warn!(
                error = %err,
                path = %plugin_root.display(),
                "failed to inspect plugin scoped temp directory"
            );
            return summary;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                summary.errors += 1;
                warn!(
                    error = %err,
                    path = %plugin_root.display(),
                    "failed to inspect plugin temp entry"
                );
                continue;
            }
        };
        match stale_temp_entry(&entry, now, max_age) {
            Ok(true) => match std::fs::remove_dir_all(entry.path()) {
                Ok(()) => {
                    summary.removed_dirs += 1;
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    summary.errors += 1;
                    warn!(
                        error = %err,
                        path = %entry.path().display(),
                        "failed to remove stale plugin temp directory"
                    );
                }
            },
            Ok(false) => {
                summary.kept_entries += 1;
            }
            Err(err) => {
                summary.errors += 1;
                warn!(
                    error = %err,
                    path = %entry.path().display(),
                    "failed to inspect plugin temp entry metadata"
                );
            }
        }
    }

    summary
}

fn stale_temp_entry(
    entry: &std::fs::DirEntry,
    now: SystemTime,
    max_age: Duration,
) -> Result<bool, std::io::Error> {
    let file_type = entry.file_type()?;
    if !file_type.is_dir() || file_type.is_symlink() {
        return Ok(false);
    }

    let modified = entry.metadata()?.modified()?;
    Ok(modified_time_is_stale(modified, now, max_age))
}

fn modified_time_is_stale(modified: SystemTime, now: SystemTime, max_age: Duration) -> bool {
    now.duration_since(modified).is_ok_and(|age| age > max_age)
}

fn cleanup_plugin_temp_dir(path: &Path) {
    match std::fs::remove_dir_all(path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            warn!(
                error = %err,
                path = %path.display(),
                "failed to remove plugin scoped temp directory"
            );
        }
    }
}

fn validate_plugin_dir_name(plugin_id: &str) -> Result<(), PluginWasiError> {
    let value = plugin_id.trim();
    if value.is_empty() {
        return Err(PluginWasiError::Entrypoint(
            "plugin id must not be empty for WASI directory scoping".to_owned(),
        ));
    }
    if value.starts_with(['.', '_', '-']) || value.ends_with(['.', '_', '-']) {
        return Err(invalid_plugin_dir_name());
    }
    if value.contains("..")
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
    {
        return Err(invalid_plugin_dir_name());
    }
    Ok(())
}

fn sanitize_dispatch_segment(value: &str) -> Result<String, PluginWasiError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 {
        return Err(PluginWasiError::Entrypoint(
            "dispatch id must be 1 to 128 characters for WASI temp scoping".to_owned(),
        ));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(PluginWasiError::Entrypoint(
            "dispatch id contains unsupported characters for WASI temp scoping".to_owned(),
        ));
    }
    Ok(value.to_ascii_lowercase())
}

fn unix_timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn invalid_plugin_dir_name() -> PluginWasiError {
    PluginWasiError::Entrypoint(
        "plugin id contains unsupported characters for WASI directory scoping".to_owned(),
    )
}

fn resolve_wasi_entrypoint(
    package_root: &Path,
    entrypoint: &str,
) -> Result<PathBuf, PluginWasiError> {
    let mut path = package_root.to_path_buf();
    let relative = Path::new(entrypoint.trim());
    for component in relative.components() {
        match component {
            Component::Normal(segment) => path.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PluginWasiError::Entrypoint(
                    "WASI entrypoint must stay inside the extracted plugin package".to_owned(),
                ));
            }
        }
    }

    if path == package_root {
        return Err(PluginWasiError::Entrypoint(
            "WASI entrypoint must point to a wasm file".to_owned(),
        ));
    }
    Ok(path)
}

fn validate_wasi_module_file(path: &Path, max_module_bytes: u64) -> Result<(), PluginWasiError> {
    let metadata = std::fs::metadata(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            return PluginWasiError::Entrypoint("WASI entrypoint file does not exist".to_owned());
        }
        PluginWasiError::Io(format!("WASI entrypoint file cannot be inspected: {err}"))
    })?;
    if !metadata.is_file() {
        return Err(PluginWasiError::Entrypoint(
            "WASI entrypoint must point to a file".to_owned(),
        ));
    }
    if metadata.len() == 0 {
        return Err(PluginWasiError::Entrypoint(
            "WASI entrypoint file must not be empty".to_owned(),
        ));
    }
    if metadata.len() > max_module_bytes {
        return Err(PluginWasiError::Entrypoint(format!(
            "WASI entrypoint file exceeds {max_module_bytes} bytes"
        )));
    }
    Ok(())
}

fn epoch_deadline_ticks(timeout: Duration) -> u64 {
    let timeout_ms = timeout.as_millis().max(1);
    let tick_ms = u128::from(WASI_EPOCH_TICK_MILLIS);
    let ticks = timeout_ms.div_ceil(tick_ms);
    ticks.min(u128::from(u64::MAX)) as u64
}

fn is_epoch_interruption(error: &wasmtime::Error) -> bool {
    matches!(error.downcast_ref::<Trap>(), Some(Trap::Interrupt))
}

fn captured_stdout(pipe: &MemoryOutputPipe) -> String {
    String::from_utf8_lossy(&pipe.contents()).to_string()
}

fn wasi_response_body(stdout: &str, stderr: &str) -> Option<String> {
    let body = match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => return None,
        (false, true) => stdout.to_owned(),
        (true, false) => format!("stderr:\n{stderr}"),
        (false, false) => format!("stdout:\n{stdout}\nstderr:\n{stderr}"),
    };
    Some(truncate_str(&body, MAX_WASI_AUDIT_BYTES))
}

fn wasi_error_suffix(stdout: &str, stderr: &str) -> String {
    wasi_response_body(stdout, stderr)
        .map(|body| format!("; captured output: {body}"))
        .unwrap_or_default()
}

fn truncate_str(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }

    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

impl Display for PluginWasiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Entrypoint(message) => write!(f, "plugin WASI entrypoint error: {message}"),
            Self::Io(message) => write!(f, "plugin WASI I/O error: {message}"),
            Self::Compile(message) => write!(f, "plugin WASI compile error: {message}"),
            Self::Runtime(message) => write!(f, "plugin WASI runtime error: {message}"),
            Self::Timeout => f.write_str("plugin WASI execution timed out"),
            Self::Join(message) => write!(f, "plugin WASI execution task failed: {message}"),
            Self::Exit {
                code,
                stdout,
                stderr,
            } => write!(
                f,
                "plugin WASI exited with status {code}{}",
                wasi_error_suffix(stdout, stderr)
            ),
        }
    }
}

impl Error for PluginWasiError {}

#[cfg(test)]
mod tests {
    use std::{
        fs as std_fs, process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use serde_json::json;

    use super::*;

    #[tokio::test]
    #[ignore = "run after: (cd examples/plugins/wasi-scan-logger-template && cargo build --release --target wasm32-wasip1); then `cargo test -- --ignored wasi_scan_logger`"]
    async fn wasi_scan_logger_template_executes_end_to_end() {
        let template_release = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples/plugins/wasi-scan-logger-template/target/wasm32-wasip1/release");
        let wasm = template_release.join("plugin.wasm");
        assert!(
            wasm.exists(),
            "build the template first (cargo build --release --target wasm32-wasip1): {}",
            wasm.display()
        );

        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base =
            std::env::temp_dir().join(format!("fbz-wasi-smoke-{}-{}", process::id(), now_nanos));
        let data = base.join("data");
        let cache = base.join("cache");
        let tmp = base.join("tmp");
        for dir in [&data, &cache, &tmp] {
            std_fs::create_dir_all(dir).unwrap();
        }

        // Stage the built wasm in the layout the runtime resolves:
        // {package_dir}/extracted/{plugin_id}/{version}/plugin.wasm
        let package_root = base
            .join(PLUGIN_PACKAGE_EXTRACTED_DIR)
            .join("dev.fbz.wasi.scan-logger")
            .join("0.1.0");
        std_fs::create_dir_all(&package_root).unwrap();
        std_fs::copy(&wasm, package_root.join("plugin.wasm")).unwrap();

        let output = PluginWasiRuntime::new()
            .execute(PluginWasiExecution {
                package_dir: base.clone(),
                data_dir: data,
                cache_dir: cache,
                tmp_dir: tmp,
                plugin_id: "dev.fbz.wasi.scan-logger".to_owned(),
                package_version: "0.1.0".to_owned(),
                entrypoint: "plugin.wasm".to_owned(),
                handler: "hooks.onScanCompleted".to_owned(),
                idempotency_key: "smoke-1".to_owned(),
                host_base_url: "http://127.0.0.1:8080".to_owned(),
                host_token: None,
                payload: json!({
                    "eventType": "library.scan.completed",
                    "aggregateId": "lib-1"
                }),
                timeout: Duration::from_secs(5),
                memory_limit_mb: 128,
                fuel: 100_000_000,
                stdio_max_bytes: 65536,
                max_module_bytes: 67_108_864,
                tmp_max_age: Duration::from_secs(86_400),
            })
            .await
            .expect("wasi template execution should succeed");

        let body = output.response_body.expect("response body on stdout");
        // The template echoes its dispatch context and the received event payload.
        assert!(body.contains("dev.fbz.wasi.scan-logger"), "body: {body}");
        assert!(body.contains("hooks.onScanCompleted"), "body: {body}");
        assert!(body.contains("library.scan.completed"), "body: {body}");

        let _ = std_fs::remove_dir_all(&base);
    }

    #[test]
    fn wasi_entrypoint_path_stays_inside_package() {
        let root = PathBuf::from("/tmp/plugin");

        assert_eq!(
            resolve_wasi_entrypoint(&root, "bin/plugin.wasm").unwrap(),
            root.join("bin").join("plugin.wasm")
        );
        assert!(resolve_wasi_entrypoint(&root, "../plugin.wasm").is_err());
        assert!(resolve_wasi_entrypoint(&root, "/plugin.wasm").is_err());
        assert!(resolve_wasi_entrypoint(&root, "").is_err());
    }

    #[test]
    fn plugin_scoped_dir_rejects_path_like_plugin_ids() {
        let root = PathBuf::from("/tmp/plugin-data");

        assert_eq!(
            plugin_scoped_dir(&root, "dev.fbz.notify").unwrap(),
            root.join("dev.fbz.notify")
        );
        assert!(plugin_scoped_dir(&root, "../escape").is_err());
        assert!(plugin_scoped_dir(&root, "Dev.FBZ.Notify").is_err());
        assert!(plugin_scoped_dir(&root, "dev/fbz/notify").is_err());
    }

    #[test]
    fn wasi_response_body_preserves_stdout_and_stderr_boundary() {
        assert_eq!(wasi_response_body("ok\n", ""), Some("ok\n".to_owned()));
        assert_eq!(
            wasi_response_body("ok\n", "warn\n"),
            Some("stdout:\nok\n\nstderr:\nwarn\n".to_owned())
        );
        assert_eq!(wasi_response_body("", ""), None);
        assert_eq!(truncate_str("错误错误错误", 7), "错误");
    }

    #[test]
    fn epoch_deadline_ticks_rounds_up_to_tick_boundary() {
        assert_eq!(epoch_deadline_ticks(Duration::ZERO), 1);
        assert_eq!(
            epoch_deadline_ticks(Duration::from_millis(WASI_EPOCH_TICK_MILLIS)),
            1
        );
        assert_eq!(
            epoch_deadline_ticks(Duration::from_millis(WASI_EPOCH_TICK_MILLIS + 1)),
            2
        );
    }

    #[test]
    fn modified_time_stale_check_requires_age_over_limit() {
        let now = UNIX_EPOCH + Duration::from_secs(20);

        assert!(modified_time_is_stale(
            UNIX_EPOCH,
            now,
            Duration::from_secs(19)
        ));
        assert!(!modified_time_is_stale(
            UNIX_EPOCH,
            now,
            Duration::from_secs(20)
        ));
        assert!(!modified_time_is_stale(
            now,
            UNIX_EPOCH,
            Duration::from_secs(1)
        ));
    }

    #[test]
    fn stale_plugin_temp_scavenging_removes_expired_directories_and_keeps_files() {
        let base_dir = unique_test_dir("fbz-wasi-temp-scavenge-test");
        let plugin_root = base_dir.join("tmp").join("dev.fbz.wasi");
        let stale_dir = plugin_root.join("dispatch-old");
        let fresh_dir = plugin_root.join("dispatch-fresh");
        let direct_file = plugin_root.join("note.txt");
        std_fs::create_dir_all(&stale_dir).unwrap();
        std_fs::create_dir_all(&fresh_dir).unwrap();
        std_fs::write(&direct_file, "keep").unwrap();

        let summary = cleanup_stale_plugin_temp_dirs_at(
            &plugin_root,
            Duration::from_secs(1),
            SystemTime::now() + Duration::from_secs(60),
        );

        assert_eq!(summary.removed_dirs, 2);
        assert_eq!(summary.kept_entries, 1);
        assert_eq!(summary.errors, 0);
        assert!(!stale_dir.exists());
        assert!(!fresh_dir.exists());
        assert!(direct_file.exists());

        std_fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn stale_plugin_temp_scavenging_keeps_recent_directories() {
        let base_dir = unique_test_dir("fbz-wasi-temp-recent-test");
        let plugin_root = base_dir.join("tmp").join("dev.fbz.wasi");
        let recent_dir = plugin_root.join("dispatch-recent");
        std_fs::create_dir_all(&recent_dir).unwrap();

        let summary = cleanup_stale_plugin_temp_dirs_at(
            &plugin_root,
            Duration::from_secs(60 * 60),
            SystemTime::now(),
        );

        assert_eq!(summary.removed_dirs, 0);
        assert_eq!(summary.kept_entries, 1);
        assert_eq!(summary.errors, 0);
        assert!(recent_dir.exists());

        std_fs::remove_dir_all(base_dir).unwrap();
    }

    #[tokio::test]
    async fn wasi_runtime_executes_preview1_command() {
        let base_dir = unique_test_dir("fbz-wasi-runtime-test");
        let plugin_root = base_dir
            .join(PLUGIN_PACKAGE_EXTRACTED_DIR)
            .join("dev.fbz.wasi")
            .join("1.0.0");
        std_fs::create_dir_all(&plugin_root).unwrap();
        let wasm = wat::parse_str(
            r#"
            (module
              (import "wasi_snapshot_preview1" "fd_write"
                (func $fd_write (param i32 i32 i32 i32) (result i32)))
              (memory (export "memory") 1)
              (data (i32.const 8) "ok\0a")
              (func $_start (export "_start")
                (i32.store (i32.const 0) (i32.const 8))
                (i32.store (i32.const 4) (i32.const 3))
                (drop (call $fd_write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 20))))
            )
            "#,
        )
        .unwrap();
        std_fs::write(plugin_root.join("plugin.wasm"), wasm).unwrap();

        let output = PluginWasiRuntime::new()
            .execute(PluginWasiExecution {
                package_dir: base_dir.clone(),
                data_dir: base_dir.join("data"),
                cache_dir: base_dir.join("cache"),
                tmp_dir: base_dir.join("tmp"),
                plugin_id: "dev.fbz.wasi".to_owned(),
                package_version: "1.0.0".to_owned(),
                entrypoint: "plugin.wasm".to_owned(),
                handler: "hooks.onTest".to_owned(),
                idempotency_key: "dispatch-1".to_owned(),
                host_base_url: "http://127.0.0.1:8080".to_owned(),
                host_token: None,
                payload: json!({"hello": "world"}),
                timeout: Duration::from_secs(5),
                memory_limit_mb: 16,
                fuel: 1_000_000,
                stdio_max_bytes: 1024,
                max_module_bytes: 64 * 1024,
                tmp_max_age: Duration::from_secs(60 * 60),
            })
            .await
            .unwrap();

        assert_eq!(output.response_body, Some("ok\n".to_owned()));

        std_fs::remove_dir_all(base_dir).unwrap();
    }

    #[tokio::test]
    async fn wasi_runtime_interrupts_cpu_bound_modules_on_timeout() {
        let base_dir = unique_test_dir("fbz-wasi-timeout-test");
        let plugin_root = base_dir
            .join(PLUGIN_PACKAGE_EXTRACTED_DIR)
            .join("dev.fbz.wasi")
            .join("1.0.0");
        std_fs::create_dir_all(&plugin_root).unwrap();
        let wasm = wat::parse_str(
            r#"
            (module
              (func $_start (export "_start")
                (loop $again
                  br $again))
            )
            "#,
        )
        .unwrap();
        std_fs::write(plugin_root.join("plugin.wasm"), wasm).unwrap();

        let err = PluginWasiRuntime::new()
            .execute(PluginWasiExecution {
                package_dir: base_dir.clone(),
                data_dir: base_dir.join("data"),
                cache_dir: base_dir.join("cache"),
                tmp_dir: base_dir.join("tmp"),
                plugin_id: "dev.fbz.wasi".to_owned(),
                package_version: "1.0.0".to_owned(),
                entrypoint: "plugin.wasm".to_owned(),
                handler: "hooks.onLoop".to_owned(),
                idempotency_key: "dispatch-timeout".to_owned(),
                host_base_url: "http://127.0.0.1:8080".to_owned(),
                host_token: None,
                payload: json!({"hello": "world"}),
                timeout: Duration::from_millis(WASI_EPOCH_TICK_MILLIS * 3),
                memory_limit_mb: 16,
                fuel: u64::MAX,
                stdio_max_bytes: 1024,
                max_module_bytes: 64 * 1024,
                tmp_max_age: Duration::from_secs(60 * 60),
            })
            .await
            .unwrap_err();

        assert!(matches!(err, PluginWasiError::Timeout));
        let plugin_tmp_dir = base_dir.join("tmp").join("dev.fbz.wasi");
        assert!(
            !plugin_tmp_dir.exists() || std_fs::read_dir(plugin_tmp_dir).unwrap().count() == 0,
            "dispatch temp directory should be cleaned after timeout"
        );

        std_fs::remove_dir_all(base_dir).unwrap();
    }

    #[tokio::test]
    async fn wasi_runtime_mounts_plugin_data_and_cache_dirs() {
        let base_dir = unique_test_dir("fbz-wasi-mount-test");
        let package_dir = base_dir.join("packages");
        let data_dir = base_dir.join("data");
        let cache_dir = base_dir.join("cache");
        let tmp_dir = base_dir.join("tmp");
        let plugin_root = package_dir
            .join(PLUGIN_PACKAGE_EXTRACTED_DIR)
            .join("dev.fbz.wasi")
            .join("1.0.0");
        std_fs::create_dir_all(&plugin_root).unwrap();
        let wasm = wat::parse_str(
            r#"
            (module
              (import "wasi_snapshot_preview1" "path_open"
                (func $path_open
                  (param i32 i32 i32 i32 i32 i64 i64 i32 i32)
                  (result i32)))
              (import "wasi_snapshot_preview1" "fd_write"
                (func $fd_write (param i32 i32 i32 i32) (result i32)))
              (memory (export "memory") 1)
              (data (i32.const 8) "state.txt")
              (data (i32.const 64) "saved\0a")
              (data (i32.const 96) "scratch.txt")
              (data (i32.const 128) "temp\0a")
              (func $_start (export "_start")
                (drop
                  (call $path_open
                    (i32.const 4)
                    (i32.const 0)
                    (i32.const 8)
                    (i32.const 9)
                    (i32.const 1)
                    (i64.const 64)
                    (i64.const 0)
                    (i32.const 0)
                    (i32.const 40)))
                (i32.store (i32.const 0) (i32.const 64))
                (i32.store (i32.const 4) (i32.const 6))
                (drop
                  (call $fd_write
                    (i32.load (i32.const 40))
                    (i32.const 0)
                    (i32.const 1)
                    (i32.const 48)))
                (drop
                  (call $path_open
                    (i32.const 6)
                    (i32.const 0)
                    (i32.const 96)
                    (i32.const 11)
                    (i32.const 1)
                    (i64.const 64)
                    (i64.const 0)
                    (i32.const 0)
                    (i32.const 56)))
                (i32.store (i32.const 72) (i32.const 128))
                (i32.store (i32.const 76) (i32.const 5))
                (drop
                  (call $fd_write
                    (i32.load (i32.const 56))
                    (i32.const 72)
                    (i32.const 1)
                    (i32.const 88))))
            )
            "#,
        )
        .unwrap();
        std_fs::write(plugin_root.join("plugin.wasm"), wasm).unwrap();

        PluginWasiRuntime::new()
            .execute(PluginWasiExecution {
                package_dir,
                data_dir: data_dir.clone(),
                cache_dir: cache_dir.clone(),
                tmp_dir: tmp_dir.clone(),
                plugin_id: "dev.fbz.wasi".to_owned(),
                package_version: "1.0.0".to_owned(),
                entrypoint: "plugin.wasm".to_owned(),
                handler: "hooks.onTest".to_owned(),
                idempotency_key: "dispatch-1".to_owned(),
                host_base_url: "http://127.0.0.1:8080".to_owned(),
                host_token: None,
                payload: json!({"hello": "world"}),
                timeout: Duration::from_secs(5),
                memory_limit_mb: 16,
                fuel: 1_000_000,
                stdio_max_bytes: 1024,
                max_module_bytes: 64 * 1024,
                tmp_max_age: Duration::from_secs(60 * 60),
            })
            .await
            .unwrap();

        assert_eq!(
            std_fs::read_to_string(data_dir.join("dev.fbz.wasi").join("state.txt")).unwrap(),
            "saved\n"
        );
        assert!(cache_dir.join("dev.fbz.wasi").is_dir());
        let plugin_tmp_dir = tmp_dir.join("dev.fbz.wasi");
        assert!(plugin_tmp_dir.is_dir());
        assert_eq!(
            std_fs::read_dir(plugin_tmp_dir).unwrap().count(),
            0,
            "dispatch-scoped WASI temp directories should be removed after execution"
        );

        std_fs::remove_dir_all(base_dir).unwrap();
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{}-{}-{}",
            prefix,
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
