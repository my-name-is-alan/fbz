use axum::{body::Bytes, http::HeaderMap};
use http::header::CONTENT_TYPE;
use serde::de::DeserializeOwned;

use crate::error::AppError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmbyBodyFormat {
    Json,
    Xml,
}

pub fn parse_emby_body<T>(headers: &HeaderMap, body: &Bytes) -> Result<T, AppError>
where
    T: DeserializeOwned,
{
    if body.is_empty() {
        return Err(AppError::unprocessable("request body is required"));
    }

    match emby_body_format(headers)? {
        EmbyBodyFormat::Json => serde_json::from_slice(body)
            .map_err(|err| AppError::unprocessable(format!("invalid JSON request body: {err}"))),
        EmbyBodyFormat::Xml => {
            let body = std::str::from_utf8(body).map_err(|err| {
                AppError::unprocessable(format!("invalid XML request body encoding: {err}"))
            })?;
            quick_xml::de::from_str(body)
                .map_err(|err| AppError::unprocessable(format!("invalid XML request body: {err}")))
        }
    }
}

fn emby_body_format(headers: &HeaderMap) -> Result<EmbyBodyFormat, AppError> {
    let Some(content_type) = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(normalize_content_type)
    else {
        return Ok(EmbyBodyFormat::Json);
    };

    if is_json_content_type(&content_type) {
        return Ok(EmbyBodyFormat::Json);
    }

    if is_xml_content_type(&content_type) {
        return Ok(EmbyBodyFormat::Xml);
    }

    Err(AppError::unprocessable(format!(
        "unsupported request content type: {content_type}"
    )))
}

fn normalize_content_type(value: &str) -> String {
    value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn is_json_content_type(content_type: &str) -> bool {
    content_type == "application/json" || content_type.ends_with("+json")
}

fn is_xml_content_type(content_type: &str) -> bool {
    matches!(content_type, "application/xml" | "text/xml") || content_type.ends_with("+xml")
}

#[cfg(test)]
mod tests {
    use axum::http::header::CONTENT_TYPE;

    use crate::compat::emby::dto::{
        AuthenticateByNameRequestDto, PlaybackInfoRequestDto, PlaybackProgressDto,
    };

    use super::*;

    #[test]
    fn parses_json_when_content_type_is_missing() {
        let headers = HeaderMap::new();
        let body = Bytes::from_static(br#"{"Username":"admin","Pw":"secret"}"#);

        let payload: AuthenticateByNameRequestDto = parse_emby_body(&headers, &body).unwrap();

        assert_eq!(payload.username, "admin");
        assert_eq!(payload.password(), Some("secret"));
    }

    #[test]
    fn parses_xml_authenticate_by_name_payload() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, "application/xml".parse().unwrap());
        let body = Bytes::from_static(
            br#"<AuthenticateByNameRequestDto><Username>admin</Username><Pw>secret</Pw></AuthenticateByNameRequestDto>"#,
        );

        let payload: AuthenticateByNameRequestDto = parse_emby_body(&headers, &body).unwrap();

        assert_eq!(payload.username, "admin");
        assert_eq!(payload.password(), Some("secret"));
    }

    #[test]
    fn parses_xml_playback_info_payload() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, "text/xml; charset=utf-8".parse().unwrap());
        let body = Bytes::from_static(
            br#"<PlaybackInfoRequestDto><UserId>user-1</UserId><MaxStreamingBitrate>8000000</MaxStreamingBitrate><StartTimeTicks>100</StartTimeTicks><MediaSourceId>42</MediaSourceId><DeviceProfile><Name>client-profile</Name></DeviceProfile></PlaybackInfoRequestDto>"#,
        );

        let payload: PlaybackInfoRequestDto = parse_emby_body(&headers, &body).unwrap();

        assert_eq!(payload.user_id.as_deref(), Some("user-1"));
        assert_eq!(payload.max_streaming_bitrate, Some(8_000_000));
        assert_eq!(payload.start_time_ticks, Some(100));
        assert_eq!(payload.media_source_id.as_deref(), Some("42"));
        assert!(payload.device_profile.is_some());
    }

    #[test]
    fn parses_vendor_xml_playback_progress_payload() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, "application/vnd.emby+xml".parse().unwrap());
        let body = Bytes::from_static(
            br#"<PlaybackProgressDto><ItemId>item-1</ItemId><UserId>user-1</UserId><PlaySessionId>play-1</PlaySessionId><MediaSourceId>42</MediaSourceId><PlayMethod>DirectStream</PlayMethod><PositionTicks>42</PositionTicks><IsPaused>true</IsPaused></PlaybackProgressDto>"#,
        );

        let payload: PlaybackProgressDto = parse_emby_body(&headers, &body).unwrap();

        assert_eq!(payload.item_id, "item-1");
        assert_eq!(payload.user_id.as_deref(), Some("user-1"));
        assert_eq!(payload.play_session_id.as_deref(), Some("play-1"));
        assert_eq!(payload.media_source_id.as_deref(), Some("42"));
        assert_eq!(payload.play_method.as_deref(), Some("DirectStream"));
        assert_eq!(payload.position_ticks, Some(42));
        assert_eq!(payload.is_paused, Some(true));
    }

    #[test]
    fn rejects_unsupported_content_type() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, "text/plain".parse().unwrap());
        let body = Bytes::from_static(b"Username=admin");

        let err = parse_emby_body::<AuthenticateByNameRequestDto>(&headers, &body).unwrap_err();

        assert_eq!(err.status_code(), http::StatusCode::UNPROCESSABLE_ENTITY);
        assert!(err.message().contains("unsupported request content type"));
    }
}
