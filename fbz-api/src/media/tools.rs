use std::{
    error::Error,
    fmt::{Display, Formatter},
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;

use crate::config::MediaToolConfig;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MediaToolDiagnostics {
    pub ffmpeg: ResolvedMediaTool,
    pub ffprobe: ResolvedMediaTool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResolvedMediaTool {
    pub kind: MediaToolKind,
    pub source: MediaToolSource,
    pub path: PathBuf,
    pub version_line: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaToolKind {
    Ffmpeg,
    Ffprobe,
}

impl MediaToolKind {
    fn binary_name(self) -> &'static str {
        match self {
            Self::Ffmpeg => "ffmpeg",
            Self::Ffprobe => "ffprobe",
        }
    }

    fn env_key(self) -> &'static str {
        match self {
            Self::Ffmpeg => "FFMPEG_PATH",
            Self::Ffprobe => "FFPROBE_PATH",
        }
    }
}

impl Display for MediaToolKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.binary_name())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaToolSource {
    External,
    Bundled,
}

#[derive(Debug)]
pub enum MediaToolError {
    ExplicitPathFailed {
        kind: MediaToolKind,
        key: &'static str,
        path: PathBuf,
        message: String,
    },
    NotFound {
        kind: MediaToolKind,
        tried: Vec<PathBuf>,
    },
}

impl Display for MediaToolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExplicitPathFailed {
                kind,
                key,
                path,
                message,
            } => write!(
                f,
                "{kind} configured by {key} at `{}` is not executable: {message}",
                path.display()
            ),
            Self::NotFound { kind, tried } => {
                write!(f, "{kind} executable not found")?;
                if !tried.is_empty() {
                    write!(
                        f,
                        "; tried {}",
                        tried
                            .iter()
                            .map(|path| format!("`{}`", path.display()))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                }
                Ok(())
            }
        }
    }
}

impl Error for MediaToolError {}

pub fn resolve_media_tools(
    config: &MediaToolConfig,
) -> Result<MediaToolDiagnostics, MediaToolError> {
    Ok(MediaToolDiagnostics {
        ffmpeg: resolve_tool(
            MediaToolKind::Ffmpeg,
            &config.ffmpeg_path,
            config.ffmpeg_path_explicit,
            config,
        )?,
        ffprobe: resolve_tool(
            MediaToolKind::Ffprobe,
            &config.ffprobe_path,
            config.ffprobe_path_explicit,
            config,
        )?,
    })
}

fn resolve_tool(
    kind: MediaToolKind,
    external_path: &str,
    external_path_explicit: bool,
    config: &MediaToolConfig,
) -> Result<ResolvedMediaTool, MediaToolError> {
    let external = PathBuf::from(external_path);

    if let Ok(tool) = validate_candidate(kind, MediaToolSource::External, &external) {
        return Ok(tool);
    }

    if external_path_explicit {
        let message = version_error(&external);
        return Err(MediaToolError::ExplicitPathFailed {
            kind,
            key: kind.env_key(),
            path: external,
            message,
        });
    }

    let mut tried = vec![external];
    if config.enable_bundled {
        let bundled = bundled_path(&config.bundled_dir, kind);
        tried.push(bundled.clone());

        if let Ok(tool) = validate_candidate(kind, MediaToolSource::Bundled, &bundled) {
            return Ok(tool);
        }
    }

    Err(MediaToolError::NotFound { kind, tried })
}

fn validate_candidate(
    kind: MediaToolKind,
    source: MediaToolSource,
    path: &Path,
) -> Result<ResolvedMediaTool, String> {
    let output = Command::new(path)
        .arg("-version")
        .output()
        .map_err(|err| err.to_string())?;

    if !output.status.success() {
        return Err(format!(
            "version command exited with status {}",
            output.status
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let version_line = stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| "version command returned no output".to_owned())?
        .to_owned();

    Ok(ResolvedMediaTool {
        kind,
        source,
        path: path.to_path_buf(),
        version_line,
    })
}

fn version_error(path: &Path) -> String {
    Command::new(path)
        .arg("-version")
        .output()
        .map(|output| {
            if output.status.success() {
                "version command unexpectedly succeeded".to_owned()
            } else {
                format!("version command exited with status {}", output.status)
            }
        })
        .unwrap_or_else(|err| err.to_string())
}

fn bundled_path(root: &Path, kind: MediaToolKind) -> PathBuf {
    let file_name = if cfg!(windows) {
        format!("{}.exe", kind.binary_name())
    } else {
        kind.binary_name().to_owned()
    };

    root.join(file_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_path_uses_platform_binary_name() {
        let path = bundled_path(Path::new("./vendor/ffmpeg"), MediaToolKind::Ffmpeg);
        let expected = if cfg!(windows) {
            PathBuf::from("./vendor/ffmpeg/ffmpeg.exe")
        } else {
            PathBuf::from("./vendor/ffmpeg/ffmpeg")
        };

        assert_eq!(path, expected);
    }

    #[test]
    fn explicit_missing_path_fails_without_bundled_fallback() {
        let config = MediaToolConfig {
            ffmpeg_path: "__definitely_missing_ffmpeg__".to_owned(),
            ffmpeg_path_explicit: true,
            ffprobe_path: "ffprobe".to_owned(),
            ffprobe_path_explicit: false,
            bundled_dir: PathBuf::from("./vendor/ffmpeg"),
            enable_bundled: true,
        };

        let err = resolve_tool(
            MediaToolKind::Ffmpeg,
            &config.ffmpeg_path,
            config.ffmpeg_path_explicit,
            &config,
        )
        .unwrap_err();

        assert!(matches!(err, MediaToolError::ExplicitPathFailed { .. }));
    }
}
