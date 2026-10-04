//! Same-origin, permission-checked, bounded Range relay for browser demuxers.
use crate::{
    compat::emby::routes::access::authenticate_request_user, error::AppError,
    media::repository::MediaRepository, state::AppState,
};
use axum::{
    Router,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use std::{
    net::IpAddr,
    sync::{Arc, OnceLock},
    time::Duration,
};
const MAX_RANGE: u64 = 32 * 1024 * 1024;
static SLOTS: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
pub fn router() -> Router<AppState> {
    Router::new().route("/api/media/{id}/bytes", get(read_bytes))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    media_source_id: Option<i64>,
}
fn byte_range(value: Option<&str>, size: u64) -> Result<(u64, u64), AppError> {
    if size == 0 {
        return Err(AppError::range_not_satisfiable("unknown media size"));
    }
    let Some(value) = value else {
        return Ok((0, 1.min(size - 1)));
    };
    let raw = value
        .strip_prefix("bytes=")
        .filter(|v| !v.contains(','))
        .ok_or_else(|| AppError::range_not_satisfiable("single byte range required"))?;
    let (start, end) = raw
        .split_once('-')
        .ok_or_else(|| AppError::range_not_satisfiable("invalid range"))?;
    let start = start
        .parse::<u64>()
        .map_err(|_| AppError::range_not_satisfiable("range start required"))?;
    let end = if end.is_empty() {
        size.saturating_sub(1)
            .min(start.saturating_add(MAX_RANGE - 1))
    } else {
        end.parse::<u64>()
            .map_err(|_| AppError::range_not_satisfiable("invalid range end"))?
            .min(size - 1)
    };
    if start >= size || end < start || end - start + 1 > MAX_RANGE {
        return Err(AppError::range_not_satisfiable(
            "range exceeds 32 MiB limit",
        ));
    }
    Ok((start, end))
}
fn public_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            !v.is_private()
                && !v.is_loopback()
                && !v.is_link_local()
                && !v.is_unspecified()
                && !v.is_broadcast()
                && !v.is_multicast()
                && !v.is_documentation()
                && v.octets()[0] != 0
                && v.octets()[0] < 224
                && !(v.octets()[0] == 100 && (64..=127).contains(&v.octets()[1]))
                && !(v.octets()[0] == 198 && (18..=19).contains(&v.octets()[1]))
        }
        IpAddr::V6(v) => {
            (v.segments()[0] & 0xe000) == 0x2000
                && v.segments()[0] != 0x2002
                && !(v.segments()[0] == 0x2001 && [0, 0xdb8].contains(&v.segments()[1]))
        }
    }
}
async fn cdn_client(host: &str, attempt: usize) -> Result<reqwest::Client, AppError> {
    use std::{collections::HashMap, sync::Mutex, time::Instant};
    static CLIENTS: OnceLock<Mutex<HashMap<String, (reqwest::Client, Instant)>>> = OnceLock::new();
    let clients = CLIENTS.get_or_init(|| Mutex::new(HashMap::new()));
    if attempt == 0 {
        if let Some((client, until)) = clients
            .lock()
            .map_err(|_| AppError::internal("CDN pool unavailable"))?
            .get(host)
        {
            if *until > Instant::now() {
                return Ok(client.clone());
            }
        }
    }
    let mut addresses: Vec<_> =
        tokio::time::timeout(Duration::from_secs(4), tokio::net::lookup_host((host, 443)))
            .await
            .map_err(|_| AppError::internal("CDN DNS timeout"))?
            .map_err(|_| AppError::internal("CDN DNS unavailable"))?
            .collect();
    if addresses.is_empty() || addresses.iter().any(|a| !public_address(a.ip())) {
        return Err(AppError::forbidden("CDN resolves to a restricted address"));
    }
    addresses.sort_by_key(|a| a.is_ipv6());
    if attempt > 0 && addresses.len() > 1 {
        addresses.rotate_left(1);
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .resolve_to_addrs(host, &addresses)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| AppError::internal("CDN client unavailable"))?;
    let mut pool = clients
        .lock()
        .map_err(|_| AppError::internal("CDN pool unavailable"))?;
    pool.retain(|_, (_, until)| *until > Instant::now());
    if pool.len() >= 64 {
        pool.clear();
    }
    pool.insert(
        host.to_owned(),
        (client.clone(), Instant::now() + Duration::from_secs(60)),
    );
    Ok(client)
}

async fn fetch_range(mut url: reqwest::Url, range: &str) -> Result<reqwest::Response, AppError> {
    for _ in 0..=4 {
        if url.scheme() != "https"
            || url.port_or_known_default() != Some(443)
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(AppError::forbidden("unsafe CDN address"));
        }
        let host = url
            .host_str()
            .ok_or_else(|| AppError::forbidden("missing CDN host"))?
            .trim_matches(['[', ']']);
        // Reuse TLS connections; cached clients pin a previously validated public
        // address set, so DNS rebinding cannot redirect an existing client inward.
        let mut received = None;
        for attempt in 0..2 {
            let client = cdn_client(host, attempt).await?;
            match client
                .get(url.clone())
                .header(header::RANGE, range)
                .send()
                .await
            {
                Ok(response) => {
                    received = Some(response);
                    break;
                }
                Err(error) if attempt == 0 => {
                    tracing::warn!(
                        connect = error.is_connect(),
                        timeout = error.is_timeout(),
                        "retrying CDN connection"
                    );
                }
                Err(_) => return Err(AppError::internal("CDN read failed")),
            }
        }
        let response = received.ok_or_else(|| AppError::internal("CDN read failed"))?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| AppError::internal("CDN redirect missing location"))?;
            url = url
                .join(location)
                .map_err(|_| AppError::forbidden("invalid CDN redirect"))?;
            continue;
        }
        return Ok(response);
    }
    Err(AppError::internal("too many CDN redirects"))
}
async fn read_bytes(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(input): Query<Input>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let pool = state
        .database()
        .ok_or_else(|| AppError::internal("database unavailable"))?;
    let source = MediaRepository::new(pool.clone())
        .find_playback_media_source(user.id, &id, input.media_source_id)
        .await
        .map_err(|_| AppError::internal("media lookup failed"))?
        .ok_or_else(|| AppError::not_found("media not found"))?;
    if !source.path.starts_with("fbz-storage://") {
        return Err(AppError::unprocessable(
            "relay only supports registered cloud sources",
        ));
    }
    let size = source
        .file_size
        .filter(|v| *v > 0)
        .ok_or_else(|| AppError::unprocessable("unknown cloud media size"))? as u64;
    let (start, end) = byte_range(
        headers.get(header::RANGE).and_then(|v| v.to_str().ok()),
        size,
    )?;
    let permit = SLOTS
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(8)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| AppError::too_many_requests("media relay is busy"))?;
    let range = format!("bytes={start}-{end}");
    let url = crate::storage::relay_url(&state, source.media_file_id, false).await?;
    let mut response = fetch_range(
        reqwest::Url::parse(&url).map_err(|_| AppError::forbidden("invalid CDN address"))?,
        &range,
    )
    .await?;
    if matches!(response.status().as_u16(), 401 | 403 | 410) {
        let url = crate::storage::relay_url(&state, source.media_file_id, true).await?;
        response = fetch_range(
            reqwest::Url::parse(&url).map_err(|_| AppError::forbidden("invalid CDN address"))?,
            &range,
        )
        .await?;
    }
    let expected = format!("bytes {start}-{end}/{size}");
    if response.status() != StatusCode::PARTIAL_CONTENT
        || response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            != Some(expected.as_str())
    {
        return Err(AppError::unprocessable(
            "CDN did not honor the requested byte range",
        ));
    }
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .cloned()
        .unwrap_or(header::HeaderValue::from_static("application/octet-stream"));
    let (sender, receiver) = tokio::sync::mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(2);
    tokio::spawn(async move {
        let _permit = permit;
        let mut received = 0_u64;
        loop {
            let chunk = tokio::select! { _=sender.closed()=>break, chunk=response.chunk()=>chunk };
            match chunk {
                Ok(Some(chunk)) => {
                    received += chunk.len() as u64;
                    if received > end - start + 1 {
                        let _ = sender
                            .send(Err(std::io::Error::other("CDN range overflow")))
                            .await;
                        break;
                    }
                    if sender.send(Ok(chunk)).await.is_err() {
                        break;
                    }
                }
                Ok(None) => {
                    if received != end - start + 1 {
                        let _ = sender
                            .send(Err(std::io::Error::other("CDN range truncated")))
                            .await;
                    }
                    break;
                }
                Err(_) => {
                    let _ = sender
                        .send(Err(std::io::Error::other("CDN stream interrupted")))
                        .await;
                    break;
                }
            }
        }
    });
    Ok((
        StatusCode::PARTIAL_CONTENT,
        [
            (header::CONTENT_TYPE, content_type),
            (
                header::CONTENT_RANGE,
                header::HeaderValue::from_str(&expected).unwrap(),
            ),
            (
                header::CONTENT_LENGTH,
                header::HeaderValue::from_str(&(end - start + 1).to_string()).unwrap(),
            ),
            (
                header::ACCEPT_RANGES,
                header::HeaderValue::from_static("bytes"),
            ),
            (
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("private, no-store"),
            ),
        ],
        Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(receiver)),
    )
        .into_response())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limits_ranges() {
        assert_eq!(byte_range(Some("bytes=0-1023"), 2048).unwrap(), (0, 1023));
        assert!(byte_range(Some("bytes=0-999999999"), 1000000000).is_err());
        assert!(byte_range(Some("bytes=0-1,4-5"), 100).is_err());
        assert!(byte_range(Some("bytes=300-400"), 100).is_err());
    }
    #[test]
    fn rejects_non_public_targets() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "100.64.1.2",
            "::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!public_address(address.parse().unwrap()));
        }
        assert!(public_address("8.8.8.8".parse().unwrap()));
    }
}
