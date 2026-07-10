//! Emby 兼容 websocket 通道（`/embywebsocket`，含 Jellyfin 风格 `/socket` 别名）。
//!
//! 客户端携带 `api_key`（或 `X-Emby-Token`）连接后，按其 access token 对应的
//! 会话 public id 在进程内 [`SessionMessageHub`](crate::realtime::SessionMessageHub)
//! 注册出站通道；远程控制入口把 `Play` / `Playstate` / `GeneralCommand` 消息
//! 投递到该通道。服务器只消费客户端的 `KeepAlive`，其余入站消息按兼容边界忽略。

use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, Uri},
    response::Response,
};
use serde_json::json;

use crate::{
    auth::repository::AuthRepository, error::AppError, realtime::SessionMessageHub,
    state::AppState,
};

use super::access::{access_token_from_request, authenticate_request_user};

/// 官方客户端预期的 keep-alive 周期（秒）；客户端会按该值的一半发送 KeepAlive。
const FORCE_KEEP_ALIVE_SECONDS: u64 = 60;

pub async fn emby_websocket(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    // 复用 REST 同一套 token 认证（headers 或 query 的 api_key / X-Emby-Token）。
    authenticate_request_user(&state, &headers, &uri).await?;
    let token = access_token_from_request(&headers, uri.query())?;

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let session_id = AuthRepository::new(database.clone())
        .find_session_public_id_by_token(&token)
        .await
        .map_err(|err| AppError::internal(format!("failed to resolve session: {err}")))?;
    // 长期 API key 不绑定会话，无法作为远程控制目标；拒绝升级避免注册悬空通道。
    let Some(session_id) = session_id else {
        return Err(AppError::unprocessable(
            "websocket channel requires a device session token",
        ));
    };

    let state = state.clone();
    Ok(ws.on_upgrade(move |socket| handle_session_socket(socket, state, session_id)))
}

async fn handle_session_socket(mut socket: WebSocket, state: AppState, session_id: String) {
    let hub: &SessionMessageHub = state.session_hub();
    let (generation, mut outbound) = hub.register(&session_id);

    // 官方握手：连接建立后先下发 ForceKeepAlive，客户端据此开始定期 KeepAlive。
    let force_keep_alive = json!({
        "MessageType": "ForceKeepAlive",
        "Data": FORCE_KEEP_ALIVE_SECONDS,
    })
    .to_string();
    if socket.send(Message::Text(force_keep_alive.into())).await.is_err() {
        state.session_hub().unregister(&session_id, generation);
        return;
    }

    loop {
        tokio::select! {
            delivery = outbound.recv() => {
                match delivery {
                    Some(message) => {
                        if socket.send(Message::Text(message.into())).await.is_err() {
                            break;
                        }
                    }
                    // 同一会话被新连接替换（sender 丢弃），本连接退出。
                    None => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        if is_keep_alive_message(text.as_str())
                            && socket
                                .send(Message::Text(
                                    json!({ "MessageType": "KeepAlive" }).to_string().into(),
                                ))
                                .await
                                .is_err()
                        {
                            break;
                        }
                        // 其余入站消息（SessionsStart、ActivityLogEntryStart 等订阅
                        // 指令）按兼容边界忽略：会话状态以 REST 上报为权威。
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_))) => {}
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                }
            }
        }
    }

    state.session_hub().unregister(&session_id, generation);
}

fn is_keep_alive_message(raw: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|value| {
            value
                .get("MessageType")
                .and_then(|message_type| message_type.as_str())
                .map(|message_type| message_type.eq_ignore_ascii_case("KeepAlive"))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_alive_message_is_recognized_case_insensitively() {
        assert!(is_keep_alive_message(r#"{"MessageType":"KeepAlive"}"#));
        assert!(is_keep_alive_message(r#"{"MessageType":"keepalive"}"#));
        assert!(!is_keep_alive_message(r#"{"MessageType":"SessionsStart"}"#));
        assert!(!is_keep_alive_message("not json"));
    }
}
