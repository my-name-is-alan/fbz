use std::{
    error::Error,
    fmt::{Display, Formatter},
    io::ErrorKind,
    path::{Path, PathBuf},
};

use tokio::fs;
use tracing::{debug, info, warn};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscodeCleanupResult {
    Removed,
    NotFound,
    MissingOutputPath,
    Failed,
}

#[derive(Debug)]
pub enum TranscodeCleanupError {
    UnsafeOutputPath {
        output_path: PathBuf,
        cache_root: PathBuf,
    },
    OutputPathIsNotDirectory(PathBuf),
    Io(std::io::Error),
}

impl Display for TranscodeCleanupError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsafeOutputPath {
                output_path,
                cache_root,
            } => write!(
                f,
                "transcode output path `{}` is outside cache root `{}`",
                output_path.display(),
                cache_root.display()
            ),
            Self::OutputPathIsNotDirectory(path) => {
                write!(
                    f,
                    "transcode output path `{}` is not a directory",
                    path.display()
                )
            }
            Self::Io(err) => write!(f, "failed to clean transcode output: {err}"),
        }
    }
}

impl Error for TranscodeCleanupError {}

impl From<std::io::Error> for TranscodeCleanupError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub async fn cleanup_session_output_dir(
    cache_root: &Path,
    output_path: Option<&str>,
) -> Result<TranscodeCleanupResult, TranscodeCleanupError> {
    let Some(output_path) = output_path.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(TranscodeCleanupResult::MissingOutputPath);
    };
    let output_path = PathBuf::from(output_path);

    let metadata = match fs::metadata(&output_path).await {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Ok(TranscodeCleanupResult::NotFound);
        }
        Err(err) => return Err(TranscodeCleanupError::Io(err)),
    };

    if !metadata.is_dir() {
        return Err(TranscodeCleanupError::OutputPathIsNotDirectory(output_path));
    }

    let canonical_root = fs::canonicalize(cache_root).await?;
    let canonical_output = fs::canonicalize(&output_path).await?;
    if canonical_output == canonical_root || !canonical_output.starts_with(&canonical_root) {
        return Err(TranscodeCleanupError::UnsafeOutputPath {
            output_path: canonical_output,
            cache_root: canonical_root,
        });
    }

    fs::remove_dir_all(&canonical_output).await?;
    Ok(TranscodeCleanupResult::Removed)
}

pub async fn cleanup_session_output_dir_best_effort(
    cache_root: &Path,
    session_id: &str,
    output_path: Option<&str>,
    reason: &'static str,
) -> TranscodeCleanupResult {
    match cleanup_session_output_dir(cache_root, output_path).await {
        Ok(TranscodeCleanupResult::Removed) => {
            info!(
                session_id = %session_id,
                reason,
                "transcode output directory cleaned up"
            );
            TranscodeCleanupResult::Removed
        }
        Ok(result) => {
            debug!(
                session_id = %session_id,
                reason,
                cleanup_result = ?result,
                "transcode output cleanup skipped"
            );
            result
        }
        Err(err) => {
            warn!(
                session_id = %session_id,
                reason,
                error = %err,
                "failed to clean transcode output directory"
            );
            TranscodeCleanupResult::Failed
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[tokio::test]
    async fn cleanup_session_output_dir_removes_only_session_dir_under_cache_root() {
        let cache_root = temp_case("transcode-cleanup-remove");
        let session_dir = cache_root.join("session-1");
        fs::create_dir_all(&session_dir).unwrap();
        fs::write(session_dir.join("master.m3u8"), "#EXTM3U").unwrap();

        let result =
            cleanup_session_output_dir(&cache_root, Some(session_dir.to_string_lossy().as_ref()))
                .await
                .unwrap();

        assert_eq!(result, TranscodeCleanupResult::Removed);
        assert!(cache_root.exists());
        assert!(!session_dir.exists());

        let _ = fs::remove_dir_all(&cache_root);
    }

    #[tokio::test]
    async fn cleanup_session_output_dir_rejects_root_and_escaping_paths() {
        let base = temp_case("transcode-cleanup-confine");
        let cache_root = base.join("cache");
        let outside = base.join("outside");
        fs::create_dir_all(&cache_root).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let root_error =
            cleanup_session_output_dir(&cache_root, Some(cache_root.to_string_lossy().as_ref()))
                .await
                .unwrap_err();
        assert!(matches!(
            root_error,
            TranscodeCleanupError::UnsafeOutputPath { .. }
        ));

        let outside_error =
            cleanup_session_output_dir(&cache_root, Some(outside.to_string_lossy().as_ref()))
                .await
                .unwrap_err();
        assert!(matches!(
            outside_error,
            TranscodeCleanupError::UnsafeOutputPath { .. }
        ));
        assert!(outside.exists());

        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn cleanup_session_output_dir_treats_missing_output_as_noop() {
        let cache_root = temp_case("transcode-cleanup-missing");
        let session_dir = cache_root.join("missing-session");
        fs::create_dir_all(&cache_root).unwrap();

        let result =
            cleanup_session_output_dir(&cache_root, Some(session_dir.to_string_lossy().as_ref()))
                .await
                .unwrap();

        assert_eq!(result, TranscodeCleanupResult::NotFound);
        assert!(cache_root.exists());

        let _ = fs::remove_dir_all(&cache_root);
    }

    fn temp_case(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{name}-{}-{unique}", std::process::id()))
    }
}
