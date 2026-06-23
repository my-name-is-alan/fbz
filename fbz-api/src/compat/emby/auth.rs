use std::{collections::BTreeMap, error::Error, fmt::Display};

use axum::http::{HeaderMap, header};

use crate::error::AppError;

const X_EMBY_TOKEN: &str = "x-emby-token";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbyAuthContext {
    pub user_id: Option<String>,
    pub credential: Option<EmbyCredential>,
    pub client: EmbyClientContext,
}

impl EmbyAuthContext {
    pub fn require_credential(&self) -> Result<&EmbyCredential, EmbyAuthError> {
        self.credential
            .as_ref()
            .ok_or(EmbyAuthError::MissingCredential)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmbyCredential {
    AccessToken(String),
    ApiKey(String),
}

impl EmbyCredential {
    pub fn secret(&self) -> &str {
        match self {
            Self::AccessToken(value) | Self::ApiKey(value) => value,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EmbyClientContext {
    pub client: Option<String>,
    pub device: Option<String>,
    pub device_id: Option<String>,
    pub version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmbyAuthError {
    InvalidAuthorizationHeader,
    UnsupportedAuthorizationScheme(String),
    InvalidAuthorizationPair(String),
    InvalidHeaderValue(&'static str),
    MissingCredential,
}

impl Display for EmbyAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAuthorizationHeader => f.write_str("invalid Emby authorization header"),
            Self::UnsupportedAuthorizationScheme(scheme) => {
                write!(f, "unsupported authorization scheme `{scheme}`")
            }
            Self::InvalidAuthorizationPair(pair) => {
                write!(f, "invalid Emby authorization pair `{pair}`")
            }
            Self::InvalidHeaderValue(header) => write!(f, "invalid {header} header value"),
            Self::MissingCredential => f.write_str("missing Emby access token or api key"),
        }
    }
}

impl Error for EmbyAuthError {}

impl From<EmbyAuthError> for AppError {
    fn from(error: EmbyAuthError) -> Self {
        AppError::unauthorized(error.to_string())
    }
}

pub fn parse_auth_context(
    headers: &HeaderMap,
    query: Option<&str>,
) -> Result<EmbyAuthContext, EmbyAuthError> {
    let authorization = parse_authorization_header(headers)?;
    let header_token = parse_token_header(headers)?;
    let query_api_key = query.and_then(find_api_key_query_param);

    let user_id = authorization
        .as_ref()
        .and_then(|values| values.get("UserId").cloned());

    let client = EmbyClientContext {
        client: authorization
            .as_ref()
            .and_then(|values| values.get("Client").cloned()),
        device: authorization
            .as_ref()
            .and_then(|values| values.get("Device").cloned()),
        device_id: authorization
            .as_ref()
            .and_then(|values| values.get("DeviceId").cloned()),
        version: authorization
            .as_ref()
            .and_then(|values| values.get("Version").cloned()),
    };

    let credential = header_token
        .or_else(|| {
            authorization
                .as_ref()
                .and_then(|values| values.get("Token").cloned())
                .map(EmbyCredential::AccessToken)
        })
        .or_else(|| query_api_key.map(EmbyCredential::ApiKey));

    Ok(EmbyAuthContext {
        user_id,
        credential,
        client,
    })
}

fn parse_authorization_header(
    headers: &HeaderMap,
) -> Result<Option<BTreeMap<String, String>>, EmbyAuthError> {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Ok(None);
    };

    let raw = value
        .to_str()
        .map_err(|_| EmbyAuthError::InvalidHeaderValue("Authorization"))?
        .trim();

    if raw.is_empty() {
        return Err(EmbyAuthError::InvalidAuthorizationHeader);
    }

    let (scheme, rest) = raw
        .split_once(char::is_whitespace)
        .ok_or(EmbyAuthError::InvalidAuthorizationHeader)?;

    if !scheme.eq_ignore_ascii_case("Emby") {
        return Err(EmbyAuthError::UnsupportedAuthorizationScheme(
            scheme.to_owned(),
        ));
    }

    Ok(Some(parse_emby_pairs(rest)?))
}

fn parse_token_header(headers: &HeaderMap) -> Result<Option<EmbyCredential>, EmbyAuthError> {
    let Some(value) = headers.get(X_EMBY_TOKEN) else {
        return Ok(None);
    };

    let token = value
        .to_str()
        .map_err(|_| EmbyAuthError::InvalidHeaderValue("X-Emby-Token"))?
        .trim();

    if token.is_empty() {
        return Err(EmbyAuthError::MissingCredential);
    }

    Ok(Some(EmbyCredential::AccessToken(token.to_owned())))
}

fn parse_emby_pairs(input: &str) -> Result<BTreeMap<String, String>, EmbyAuthError> {
    let mut values = BTreeMap::new();

    for pair in split_quoted(input, ',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }

        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| EmbyAuthError::InvalidAuthorizationPair(pair.to_owned()))?;
        let key = key.trim();
        if key.is_empty() {
            return Err(EmbyAuthError::InvalidAuthorizationPair(pair.to_owned()));
        }

        values.insert(key.to_owned(), unquote(value.trim()));
    }

    Ok(values)
}

fn split_quoted(input: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        match ch {
            '\\' if in_quotes => {
                current.push(ch);
                escaped = true;
            }
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            value if value == delimiter && !in_quotes => {
                parts.push(current);
                current = String::new();
            }
            _ => current.push(ch),
        }
    }

    parts.push(current);
    parts
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        return trimmed[1..trimmed.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");
    }

    trimmed.to_owned()
}

fn find_api_key_query_param(query: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        key.eq_ignore_ascii_case("api_key")
            .then(|| percent_decode(value))
            .filter(|value| !value.is_empty())
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                if let Ok(decoded) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                    output.push(decoded);
                    index += 3;
                } else {
                    output.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&output).into_owned()
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn parses_emby_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static(
                r#"Emby UserId="user-1", Client="Infuse", Device="Apple TV", DeviceId="device-1", Version="8.0", Token="token-1""#,
            ),
        );

        let context = parse_auth_context(&headers, None).unwrap();

        assert_eq!(context.user_id.as_deref(), Some("user-1"));
        assert_eq!(
            context.credential,
            Some(EmbyCredential::AccessToken("token-1".to_owned()))
        );
        assert_eq!(context.client.client.as_deref(), Some("Infuse"));
        assert_eq!(context.client.device.as_deref(), Some("Apple TV"));
        assert_eq!(context.client.device_id.as_deref(), Some("device-1"));
        assert_eq!(context.client.version.as_deref(), Some("8.0"));
    }

    #[test]
    fn x_emby_token_overrides_authorization_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static(r#"Emby Token="authorization-token""#),
        );
        headers.insert(X_EMBY_TOKEN, HeaderValue::from_static("header-token"));

        let context = parse_auth_context(&headers, None).unwrap();

        assert_eq!(
            context.credential,
            Some(EmbyCredential::AccessToken("header-token".to_owned()))
        );
    }

    #[test]
    fn supports_api_key_query_parameter() {
        let headers = HeaderMap::new();

        let context = parse_auth_context(&headers, Some("foo=bar&api_key=abc%20123")).unwrap();

        assert_eq!(
            context.credential,
            Some(EmbyCredential::ApiKey("abc 123".to_owned()))
        );
    }

    #[test]
    fn missing_credentials_can_be_required_by_protected_routes() {
        let headers = HeaderMap::new();
        let context = parse_auth_context(&headers, None).unwrap();

        assert_eq!(
            context.require_credential().unwrap_err(),
            EmbyAuthError::MissingCredential
        );
    }

    #[test]
    fn unsupported_authorization_scheme_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer token"),
        );

        let err = parse_auth_context(&headers, None).unwrap_err();

        assert!(matches!(
            err,
            EmbyAuthError::UnsupportedAuthorizationScheme(_)
        ));
    }

    #[test]
    fn invalid_emby_pair_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static(r#"Emby Client="Infuse", Broken"#),
        );

        let err = parse_auth_context(&headers, None).unwrap_err();

        assert!(matches!(err, EmbyAuthError::InvalidAuthorizationPair(_)));
    }
}
