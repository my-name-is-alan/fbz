//! Library Monitor — path-change → incremental `library.scan` (minimal).
//!
//! Tonight: **polling** fingerprint of enabled library roots gated by
//! `libraries.enable_realtime_monitor` (Emby `LibraryOptions.EnableRealtimeMonitor`).
//! On change (after debounce), enqueue existing `library.scan` via
//! [`AdminRepository::queue_library_scan`] (already de-dupes queued/running).
//!
//! **notify stub:** [`NotifyBackend`] documents the future `notify` / FSEvents
//! path. Default backend is [`PollingBackend`]. No OS watcher dependency yet.
//!
//! See `docs/plans/library-monitor-draft.md` and architecture §6.3.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use sqlx::Row;
use tokio::{
    sync::broadcast,
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tracing::{debug, info, warn};

use crate::{
    admin::repository::{AdminRepository, QueueLibraryScanInput},
    config::LibraryMonitorConfig,
    db::DbPool,
};

/// System user id used when monitor enqueues scans (no interactive admin).
const MONITOR_REQUESTED_BY_USER_ID: i64 = 0;
const MONITOR_REASON: &str = "realtime_monitor";
/// Cap entries fingerprinted per root per tick (shallow + one level).
const MAX_FINGERPRINT_ENTRIES: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonitoredLibrary {
    pub public_id: String,
    pub paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PathFingerprint {
    /// Stable digest: entry count + max mtime millis + path-bytes xor-fold.
    pub token: u64,
}

impl PathFingerprint {
    pub fn empty() -> Self {
        Self { token: 0 }
    }

    /// Shallow fingerprint: root metadata + immediate children (files + dirs).
    /// Enough to notice adds/removes/renames at the library root without a full walk.
    pub fn from_roots(roots: &[PathBuf]) -> Self {
        let mut token: u64 = 0xcbf29ce484222325;
        let mut counted = 0usize;
        for root in roots {
            mix_path(&mut token, root);
            match fingerprint_dir_shallow(root, &mut token, &mut counted) {
                Ok(()) => {}
                Err(err) => {
                    // Missing root still changes the token via path mix only.
                    debug!(path = %root.display(), error = %err, "library monitor root unreadable");
                }
            }
            if counted >= MAX_FINGERPRINT_ENTRIES {
                break;
            }
        }
        Self { token }
    }
}

fn mix_path(token: &mut u64, path: &Path) {
    for b in path.to_string_lossy().as_bytes() {
        *token ^= u64::from(*b);
        *token = token.wrapping_mul(0x100000001b3);
    }
}

fn mix_u64(token: &mut u64, value: u64) {
    *token ^= value;
    *token = token.wrapping_mul(0x100000001b3);
}

fn fingerprint_dir_shallow(
    root: &Path,
    token: &mut u64,
    counted: &mut usize,
) -> Result<(), std::io::Error> {
    let meta = fs::metadata(root)?;
    if let Ok(modified) = meta.modified() {
        mix_u64(token, system_time_millis(modified));
    }
    if !meta.is_dir() {
        *counted += 1;
        return Ok(());
    }

    let mut children: Vec<PathBuf> = fs::read_dir(root)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    children.sort();
    for child in children {
        if *counted >= MAX_FINGERPRINT_ENTRIES {
            break;
        }
        mix_path(token, &child);
        if let Ok(child_meta) = fs::metadata(&child) {
            if let Ok(modified) = child_meta.modified() {
                mix_u64(token, system_time_millis(modified));
            }
            mix_u64(token, child_meta.len());
        }
        *counted += 1;
    }
    Ok(())
}

fn system_time_millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Future OS watcher seam. Tonight only [`PollingBackend`] is live.
pub trait NotifyBackend: Send {
    fn name(&self) -> &'static str;
}

/// Polling stub — used until `notify` / FSEvents is opted in.
#[derive(Clone, Debug, Default)]
pub struct PollingBackend;

impl NotifyBackend for PollingBackend {
    fn name(&self) -> &'static str {
        "polling"
    }
}

/// Documented notify placeholder (not constructed in production yet).
#[derive(Clone, Debug, Default)]
pub struct NotifyStubBackend;

impl NotifyBackend for NotifyStubBackend {
    fn name(&self) -> &'static str {
        "notify_stub"
    }
}

#[derive(Clone, Debug)]
struct WatchState {
    fingerprint: PathFingerprint,
    /// First time the fingerprint differed from the last stable one.
    pending_since: Option<Instant>,
    last_enqueued_at: Option<Instant>,
}

impl Default for WatchState {
    fn default() -> Self {
        Self {
            fingerprint: PathFingerprint::empty(),
            pending_since: None,
            last_enqueued_at: None,
        }
    }
}

pub fn spawn_library_monitor(
    pool: DbPool,
    config: LibraryMonitorConfig,
    mut shutdown: broadcast::Receiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let backend = PollingBackend;
        let mut tick = interval(Duration::from_secs(config.interval_seconds.max(1)));
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut watches: HashMap<String, WatchState> = HashMap::new();
        let debounce = Duration::from_secs(config.debounce_seconds.max(1));

        info!(
            backend = backend.name(),
            interval_seconds = config.interval_seconds,
            debounce_seconds = config.debounce_seconds,
            "library monitor started"
        );

        loop {
            tokio::select! {
                result = shutdown.recv() => {
                    match result {
                        Ok(()) | Err(broadcast::error::RecvError::Closed) => {
                            info!("library monitor shutdown received");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped, "library monitor shutdown receiver lagged");
                            break;
                        }
                    }
                }
                _ = tick.tick() => {
                    if let Err(err) = poll_once(&pool, &mut watches, debounce).await {
                        warn!(error = %err, "library monitor poll failed");
                    }
                }
            }
        }

        info!("library monitor stopped");
    })
}

async fn poll_once(
    pool: &DbPool,
    watches: &mut HashMap<String, WatchState>,
    debounce: Duration,
) -> Result<(), sqlx::Error> {
    let libraries = load_monitored_libraries(pool).await?;
    let active_ids: std::collections::HashSet<String> =
        libraries.iter().map(|lib| lib.public_id.clone()).collect();
    watches.retain(|id, _| active_ids.contains(id));

    let now = Instant::now();
    let admin = AdminRepository::new(pool.clone());

    for library in libraries {
        let next = PathFingerprint::from_roots(&library.paths);
        let state = watches.entry(library.public_id.clone()).or_default();

        if state.fingerprint == PathFingerprint::empty() {
            // First observation: establish baseline, do not scan.
            state.fingerprint = next;
            state.pending_since = None;
            continue;
        }

        if next == state.fingerprint {
            state.pending_since = None;
            continue;
        }

        let pending_since = *state.pending_since.get_or_insert(now);
        if now.duration_since(pending_since) < debounce {
            debug!(
                library_id = %library.public_id,
                "library monitor change pending debounce"
            );
            continue;
        }

        // Accept new fingerprint whether enqueue inserts or reuses existing job.
        match admin
            .queue_library_scan(QueueLibraryScanInput {
                library_id: library.public_id.clone(),
                requested_by_user_id: MONITOR_REQUESTED_BY_USER_ID,
                reason: Some(MONITOR_REASON.to_owned()),
            })
            .await
        {
            Ok(Some(job)) => {
                info!(
                    library_id = %library.public_id,
                    job_id = %job.id,
                    status = %job.status,
                    "library monitor enqueued library.scan"
                );
                state.fingerprint = next;
                state.pending_since = None;
                state.last_enqueued_at = Some(now);
            }
            Ok(None) => {
                warn!(
                    library_id = %library.public_id,
                    "library monitor enqueue returned no library row"
                );
                state.fingerprint = next;
                state.pending_since = None;
            }
            Err(err) => {
                warn!(
                    library_id = %library.public_id,
                    error = %err,
                    "library monitor failed to enqueue library.scan"
                );
            }
        }
    }

    Ok(())
}

async fn load_monitored_libraries(pool: &DbPool) -> Result<Vec<MonitoredLibrary>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        select
            l.public_id::text as library_id,
            lp.path
        from libraries l
        join library_paths lp on lp.library_id = l.id
        where l.enable_realtime_monitor = true
          and l.is_hidden = false
          and lp.is_enabled = true
          and not exists (
              select 1 from storage_mounts sm where sm.library_id = l.id
          )
        order by l.public_id, lp.path
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut libraries: Vec<MonitoredLibrary> = Vec::new();
    for row in rows {
        let library_id: String = row.try_get("library_id")?;
        let path: String = row.try_get("path")?;
        if let Some(existing) = libraries
            .iter_mut()
            .find(|lib| lib.public_id == library_id)
        {
            existing.paths.push(PathBuf::from(path));
        } else {
            libraries.push(MonitoredLibrary {
                public_id: library_id,
                paths: vec![PathBuf::from(path)],
            });
        }
    }
    Ok(libraries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn polling_and_notify_stub_backend_names_are_stable() {
        assert_eq!(PollingBackend.name(), "polling");
        assert_eq!(NotifyStubBackend.name(), "notify_stub");
    }

    #[test]
    fn fingerprint_changes_when_child_file_appears() {
        let dir = std::env::temp_dir().join(format!(
            "fbz-monitor-fp-{}-{}",
            std::process::id(),
            system_time_millis(SystemTime::now())
        ));
        fs::create_dir_all(&dir).unwrap();
        let roots = vec![dir.clone()];
        let before = PathFingerprint::from_roots(&roots);

        let mut file = File::create(dir.join("new-movie.mkv")).unwrap();
        file.write_all(b"x").unwrap();
        drop(file);

        let after = PathFingerprint::from_roots(&roots);
        assert_ne!(before, after);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_roots_fingerprint_is_stable() {
        assert_eq!(
            PathFingerprint::from_roots(&[]),
            PathFingerprint::from_roots(&[])
        );
    }

    #[test]
    fn monitor_reason_constant_is_stable() {
        assert_eq!(MONITOR_REASON, "realtime_monitor");
    }
}
