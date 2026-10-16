//! STRM 解析扩展点。
//!
//! 挂在现有 stream/download **鉴权通过之后**、发出 **302 之前**。
//! 解析器只产出跳转 Location，**不得中继视频体**（无 Range 代理、无 body 转发）。
//!
//! 首版 stub [`DirectUrlStrmResolver`]：直读已存 `strm_target`（或 path 内嵌 URL），
//! 再交由调用方做 allowlist；网盘临时链 / 多源解析可另实现本 trait，无需改鉴权网关。

use crate::error::AppError;

/// 一次 STRM 解析请求（鉴权与可见性已由上层完成）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrmResolveRequest<'a> {
    pub media_file_id: i64,
    /// 库内 `.strm` 路径或占位 path。
    pub path: &'a str,
    /// 扫描/导入时写入的目标 URL；优先于 path。
    pub strm_target: Option<&'a str>,
}

/// 解析结果：仅 Location，不包含媒体字节。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrmResolveResult {
    pub location: String,
}

/// STRM → 可 302 的 URL。失败时由上层映射为 4xx，不回退为代理拉流。
pub trait StrmResolver: Send + Sync {
    fn resolve(&self, request: &StrmResolveRequest<'_>) -> Result<StrmResolveResult, AppError>;
}

/// Stub：直读 STRM 内嵌 / 已存 URL，不做网盘 API、不签发上游密钥。
#[derive(Clone, Copy, Debug, Default)]
pub struct DirectUrlStrmResolver;

impl StrmResolver for DirectUrlStrmResolver {
    fn resolve(&self, request: &StrmResolveRequest<'_>) -> Result<StrmResolveResult, AppError> {
        let raw = request
            .strm_target
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                let path = request.path.trim();
                (!path.is_empty()).then_some(path)
            })
            .ok_or_else(|| AppError::unprocessable("strm target is empty"))?;

        // `.strm` 文件内容常见为单行 URL；若 path 仍是本地 strm 路径且无 target，无法直读。
        if looks_like_local_strm_path(raw) {
            return Err(AppError::unprocessable(
                "strm target URL is missing; direct-url stub does not read strm file bodies yet",
            ));
        }

        Ok(StrmResolveResult {
            location: raw.to_owned(),
        })
    }
}

fn looks_like_local_strm_path(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return false;
    }
    lower.ends_with(".strm") || (!value.contains("://") && std::path::Path::new(value).is_absolute())
}

/// 默认解析器：直读 URL stub。后续可换成链式 / 插件实现。
pub fn default_strm_resolver() -> DirectUrlStrmResolver {
    DirectUrlStrmResolver
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_url_stub_prefers_strm_target() {
        let resolver = DirectUrlStrmResolver;
        let result = resolver
            .resolve(&StrmResolveRequest {
                media_file_id: 1,
                path: "/library/movie.strm",
                strm_target: Some("http://192.168.1.10/movie.mkv"),
            })
            .unwrap();
        assert_eq!(result.location, "http://192.168.1.10/movie.mkv");
    }

    #[test]
    fn direct_url_stub_rejects_bare_strm_path_without_target() {
        let resolver = DirectUrlStrmResolver;
        let err = resolver
            .resolve(&StrmResolveRequest {
                media_file_id: 1,
                path: "/library/movie.strm",
                strm_target: None,
            })
            .unwrap_err();
        match err {
            crate::error::AppError::UnprocessableEntity { message } => {
                assert!(message.contains("strm target"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn direct_url_stub_accepts_https_in_path_field() {
        let resolver = DirectUrlStrmResolver;
        let result = resolver
            .resolve(&StrmResolveRequest {
                media_file_id: 2,
                path: "https://cdn.example.test/a.mkv",
                strm_target: None,
            })
            .unwrap();
        assert_eq!(result.location, "https://cdn.example.test/a.mkv");
    }
}
