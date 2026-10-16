//! SyncPlay 兼容路由（Jellyfin `/SyncPlay/*`）。
//!
//! 组状态见 [`crate::syncplay::SyncPlayRegistry`]（进程内、节点本地）；
//! 调用方会话由 access token 解析为会话 public id，组内广播经
//! `/embywebsocket` 的 [`SessionMessageHub`](crate::realtime::SessionMessageHub)
//! 下发 `SyncPlayGroupUpdate` / `SyncPlayCommand`。
//! 简化点：Buffering/Ready 不等待全员就绪；无 Shuffle/Repeat/队列重排。

use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    auth::repository::AuthRepository,
    error::AppError,
    state::AppState,
    syncplay::{Group, ItemSwitch, Outbound, PlaybackCommand, SyncPlayError},
};

use super::access::{access_token_from_request, authenticate_request_user};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct NewGroupRequestDto {
    #[serde(default, alias = "groupName", alias = "group_name")]
    pub group_name: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct JoinGroupRequestDto {
    #[serde(default, alias = "groupId", alias = "group_id")]
    pub group_id: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PingRequestDto {
    #[serde(default, alias = "ping")]
    pub ping: i64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SeekRequestDto {
    #[serde(default, alias = "positionTicks")]
    pub position_ticks: i64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SetNewQueueRequestDto {
    #[serde(default, alias = "playingQueue")]
    pub playing_queue: Vec<String>,
    #[serde(default, alias = "playingItemPosition")]
    pub playing_item_position: i64,
    #[serde(default, alias = "startPositionTicks")]
    pub start_position_ticks: i64,
}

/// JF `GroupInfoDto`。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct GroupInfoDto {
    pub group_id: String,
    pub group_name: String,
    pub state: String,
    pub participants: Vec<String>,
    pub last_updated_at: String,
}

impl From<&Group> for GroupInfoDto {
    fn from(g: &Group) -> Self {
        Self {
            group_id: g.id.clone(),
            group_name: g.name.clone(),
            state: g.state.as_str().to_owned(),
            participants: g.participants.iter().map(|p| p.user_name.clone()).collect(),
            last_updated_at: crate::syncplay::iso8601(g.last_updated_ms),
        }
    }
}

fn map_err(err: SyncPlayError) -> AppError {
    match err {
        SyncPlayError::GroupNotFound => AppError::not_found(err.message()),
        SyncPlayError::NotInGroup => AppError::conflict(err.message()),
        SyncPlayError::EmptyQueue => AppError::unprocessable(err.message()),
        SyncPlayError::ItemNotFound => AppError::not_found(err.message()),
    }
}

fn parse_body<T: DeserializeOwned + Default>(body: &Bytes) -> Result<T, AppError> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(T::default());
    }
    serde_json::from_slice(body)
        .map_err(|err| AppError::unprocessable(format!("invalid SyncPlay request body: {err}")))
}

fn deliver(state: &AppState, outbound: Vec<Outbound>) {
    for message in outbound {
        // 未连 websocket 的成员只能靠 REST 拉取组状态；投递失败不视为错误。
        state.session_hub().send_to_session(&message.session_id, message.payload);
    }
}

struct Caller {
    session_id: String,
    user_name: String,
}

async fn caller(state: &AppState, headers: &HeaderMap, uri: &Uri) -> Result<Caller, AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;
    let token = access_token_from_request(headers, uri.query())?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let session_id = AuthRepository::new(database.clone())
        .find_session_public_id_by_token(&token)
        .await
        .map_err(|err| AppError::internal(format!("failed to resolve session: {err}")))?
        .ok_or_else(|| AppError::unprocessable("SyncPlay requires a device session token"))?;
    Ok(Caller { session_id, user_name: user.username })
}

/// `POST /SyncPlay/New` — 创建组并加入。
pub async fn create_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<GroupInfoDto>, AppError> {
    let caller = caller(&state, &headers, &uri).await?;
    let req: NewGroupRequestDto = parse_body(&body)?;
    let (group, out) = state.syncplay().create(&caller.session_id, &caller.user_name, &req.group_name);
    deliver(&state, out);
    Ok(Json(GroupInfoDto::from(&group)))
}

/// 非 JF 官方别名：`POST /SyncPlay/CreateGroup`。
pub async fn create_group_alias(
    state: State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<GroupInfoDto>, AppError> {
    create_group(state, headers, uri, body).await
}

pub async fn join_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let caller = caller(&state, &headers, &uri).await?;
    let req: JoinGroupRequestDto = parse_body(&body)?;
    if req.group_id.trim().is_empty() {
        return Err(AppError::unprocessable("GroupId is required"));
    }
    let (_, out) = state
        .syncplay()
        .join(&caller.session_id, &caller.user_name, req.group_id.trim())
        .map_err(map_err)?;
    deliver(&state, out);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn leave_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let caller = caller(&state, &headers, &uri).await?;
    let out = state.syncplay().leave(&caller.session_id).map_err(map_err)?;
    deliver(&state, out);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn ping(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let caller = caller(&state, &headers, &uri).await?;
    let req: PingRequestDto = parse_body(&body)?;
    state.syncplay().ping(&caller.session_id, req.ping).map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /SyncPlay/List`。
pub async fn list_groups(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<GroupInfoDto>>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    Ok(Json(state.syncplay().list().iter().map(GroupInfoDto::from).collect()))
}

/// `GET /SyncPlay/{id}`。
pub async fn get_group(
    State(state): State<AppState>,
    Path(group_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<GroupInfoDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let group = state.syncplay().get(&group_id).ok_or_else(|| map_err(SyncPlayError::GroupNotFound))?;
    Ok(Json(GroupInfoDto::from(&group)))
}

pub async fn set_new_queue(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let caller = caller(&state, &headers, &uri).await?;
    let req: SetNewQueueRequestDto = parse_body(&body)?;
    let (_, out) = state
        .syncplay()
        .set_queue(&caller.session_id, &req.playing_queue, req.playing_item_position, req.start_position_ticks)
        .map_err(map_err)?;
    deliver(&state, out);
    Ok(StatusCode::NO_CONTENT)
}

async fn run_command(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
    command: PlaybackCommand,
) -> Result<StatusCode, AppError> {
    let caller = caller(state, headers, uri).await?;
    let (_, out) = state.syncplay().command(&caller.session_id, command).map_err(map_err)?;
    deliver(state, out);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn pause(State(state): State<AppState>, headers: HeaderMap, uri: Uri) -> Result<StatusCode, AppError> {
    run_command(&state, &headers, &uri, PlaybackCommand::Pause).await
}

pub async fn unpause(State(state): State<AppState>, headers: HeaderMap, uri: Uri) -> Result<StatusCode, AppError> {
    run_command(&state, &headers, &uri, PlaybackCommand::Unpause).await
}

pub async fn stop(State(state): State<AppState>, headers: HeaderMap, uri: Uri) -> Result<StatusCode, AppError> {
    run_command(&state, &headers, &uri, PlaybackCommand::Stop).await
}

pub async fn seek(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let req: SeekRequestDto = parse_body(&body)?;
    run_command(&state, &headers, &uri, PlaybackCommand::Seek(req.position_ticks)).await
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SetPlaylistItemRequestDto {
    #[serde(default, alias = "playlistItemId")]
    pub playlist_item_id: String,
}

/// `Buffering`：成员缓冲 → 播放中则全组暂停等待。
pub async fn buffering(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    _body: Bytes,
) -> Result<StatusCode, AppError> {
    let caller = caller(&state, &headers, &uri).await?;
    let (_, out) = state.syncplay().buffering(&caller.session_id).map_err(map_err)?;
    deliver(&state, out);
    Ok(StatusCode::NO_CONTENT)
}

/// `Ready`：成员就绪 → 全员就绪且需恢复时广播 Unpause。
pub async fn ready(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    _body: Bytes,
) -> Result<StatusCode, AppError> {
    let caller = caller(&state, &headers, &uri).await?;
    let (_, out) = state.syncplay().ready(&caller.session_id).map_err(map_err)?;
    deliver(&state, out);
    Ok(StatusCode::NO_CONTENT)
}

async fn switch(state: &AppState, headers: &HeaderMap, uri: &Uri, target: ItemSwitch) -> Result<StatusCode, AppError> {
    let caller = caller(state, headers, uri).await?;
    let (_, out) = state.syncplay().switch_item(&caller.session_id, target).map_err(map_err)?;
    deliver(state, out);
    Ok(StatusCode::NO_CONTENT)
}

/// JF `NextItemRequestDto` / `PreviousItemRequestDto` 带 `PlaylistItemId`：与组当前项不一致时视为过期请求
/// （如多个成员同时播完触发 NextItem），直接 204 不推进，避免一次结束跳两项。
fn is_stale_switch(state: &AppState, session_id: &str, body: &Bytes) -> bool {
    let Ok(req) = parse_body::<SetPlaylistItemRequestDto>(body) else { return false };
    let expected = req.playlist_item_id.trim();
    if expected.is_empty() {
        return false;
    }
    state.syncplay().group_of(session_id).is_some_and(|g| {
        usize::try_from(g.playing_index).ok().and_then(|i| g.playlist.get(i)).is_some_and(|e| e.playlist_item_id != expected)
    })
}

pub async fn next_item(State(state): State<AppState>, headers: HeaderMap, uri: Uri, body: Bytes) -> Result<StatusCode, AppError> {
    let who = caller(&state, &headers, &uri).await?;
    if is_stale_switch(&state, &who.session_id, &body) {
        return Ok(StatusCode::NO_CONTENT);
    }
    switch(&state, &headers, &uri, ItemSwitch::Next).await
}

pub async fn previous_item(State(state): State<AppState>, headers: HeaderMap, uri: Uri, body: Bytes) -> Result<StatusCode, AppError> {
    let who = caller(&state, &headers, &uri).await?;
    if is_stale_switch(&state, &who.session_id, &body) {
        return Ok(StatusCode::NO_CONTENT);
    }
    switch(&state, &headers, &uri, ItemSwitch::Previous).await
}

pub async fn set_playlist_item(State(state): State<AppState>, headers: HeaderMap, uri: Uri, body: Bytes) -> Result<StatusCode, AppError> {
    let req: SetPlaylistItemRequestDto = parse_body(&body)?;
    if req.playlist_item_id.trim().is_empty() {
        return Err(AppError::unprocessable("PlaylistItemId is required"));
    }
    switch(&state, &headers, &uri, ItemSwitch::PlaylistItem(req.playlist_item_id.trim().to_owned())).await
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ModeRequestDto {
    #[serde(default, alias = "mode")]
    pub mode: String,
}

/// `SetShuffleMode`：`{Mode: "Sorted"|"Shuffle"}`。
pub async fn set_shuffle_mode(State(state): State<AppState>, headers: HeaderMap, uri: Uri, body: Bytes) -> Result<StatusCode, AppError> {
    let req: ModeRequestDto = parse_body(&body)?;
    let shuffle = crate::syncplay::parse_shuffle_mode(req.mode.trim()).ok_or_else(|| AppError::unprocessable("Mode must be Sorted or Shuffle"))?;
    let caller = caller(&state, &headers, &uri).await?;
    let (_, out) = state.syncplay().set_shuffle(&caller.session_id, shuffle).map_err(map_err)?;
    deliver(&state, out);
    Ok(StatusCode::NO_CONTENT)
}

/// `SetRepeatMode`：`{Mode: "RepeatNone"|"RepeatAll"|"RepeatOne"}`。
pub async fn set_repeat_mode(State(state): State<AppState>, headers: HeaderMap, uri: Uri, body: Bytes) -> Result<StatusCode, AppError> {
    let req: ModeRequestDto = parse_body(&body)?;
    let mode = crate::syncplay::RepeatMode::parse(req.mode.trim()).ok_or_else(|| AppError::unprocessable("Mode must be RepeatNone, RepeatAll or RepeatOne"))?;
    let caller = caller(&state, &headers, &uri).await?;
    let (_, out) = state.syncplay().set_repeat(&caller.session_id, mode).map_err(map_err)?;
    deliver(&state, out);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct QueueRequestDto {
    #[serde(default, alias = "itemIds")]
    pub item_ids: Vec<String>,
    /// `Queue`（默认，追加队尾）| `QueueNext`（插到当前项后）。
    #[serde(default, alias = "mode")]
    pub mode: String,
}

/// `Queue`：`{ItemIds, Mode}`。
pub async fn queue(State(state): State<AppState>, headers: HeaderMap, uri: Uri, body: Bytes) -> Result<StatusCode, AppError> {
    let req: QueueRequestDto = parse_body(&body)?;
    let next = match req.mode.trim() {
        "" | "Queue" | "Default" => false,
        "QueueNext" => true,
        _ => return Err(AppError::unprocessable("Mode must be Queue or QueueNext")),
    };
    let caller = caller(&state, &headers, &uri).await?;
    let (_, out) = state.syncplay().queue_items(&caller.session_id, &req.item_ids, next).map_err(map_err)?;
    deliver(&state, out);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoveFromPlaylistRequestDto {
    #[serde(default, alias = "playlistItemIds")]
    pub playlist_item_ids: Vec<String>,
    #[serde(default, alias = "clearPlaylist")]
    pub clear_playlist: bool,
    #[serde(default, alias = "clearPlayingItem")]
    pub clear_playing_item: bool,
}

/// `RemoveFromPlaylist`：`{PlaylistItemIds, ClearPlaylist, ClearPlayingItem}`。
pub async fn remove_from_playlist(State(state): State<AppState>, headers: HeaderMap, uri: Uri, body: Bytes) -> Result<StatusCode, AppError> {
    let req: RemoveFromPlaylistRequestDto = parse_body(&body)?;
    if req.playlist_item_ids.is_empty() && !req.clear_playlist {
        return Err(AppError::unprocessable("PlaylistItemIds is required"));
    }
    let caller = caller(&state, &headers, &uri).await?;
    let (_, out) = state
        .syncplay()
        .remove_items(&caller.session_id, &req.playlist_item_ids, req.clear_playlist, req.clear_playing_item)
        .map_err(map_err)?;
    deliver(&state, out);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MovePlaylistItemRequestDto {
    #[serde(default, alias = "playlistItemId")]
    pub playlist_item_id: String,
    #[serde(default, alias = "newIndex")]
    pub new_index: i64,
}

/// `MovePlaylistItem`：`{PlaylistItemId, NewIndex}`。
pub async fn move_playlist_item(State(state): State<AppState>, headers: HeaderMap, uri: Uri, body: Bytes) -> Result<StatusCode, AppError> {
    let req: MovePlaylistItemRequestDto = parse_body(&body)?;
    if req.playlist_item_id.trim().is_empty() {
        return Err(AppError::unprocessable("PlaylistItemId is required"));
    }
    let caller = caller(&state, &headers, &uri).await?;
    let (_, out) = state.syncplay().move_item(&caller.session_id, req.playlist_item_id.trim(), req.new_index).map_err(map_err)?;
    deliver(&state, out);
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    #[test]
    fn queue_dtos_parse_jellyfin_bodies() {
        let q: QueueRequestDto = serde_json::from_str(r#"{"ItemIds":["a"],"Mode":"QueueNext"}"#).unwrap();
        assert_eq!((q.item_ids, q.mode.as_str()), (vec!["a".to_owned()], "QueueNext"));
        let r: RemoveFromPlaylistRequestDto = serde_json::from_str(r#"{"playlistItemIds":["x"],"clearPlaylist":true}"#).unwrap();
        assert!(r.clear_playlist && !r.clear_playing_item && r.playlist_item_ids == ["x"]);
        let m: MovePlaylistItemRequestDto = serde_json::from_str(r#"{"PlaylistItemId":"p","NewIndex":2}"#).unwrap();
        assert_eq!((m.playlist_item_id.as_str(), m.new_index), ("p", 2));
    }

    use super::*;

    #[test]
    fn mode_dto_accepts_both_casings() {
        let a: ModeRequestDto = serde_json::from_str(r#"{"Mode":"Shuffle"}"#).unwrap();
        let b: ModeRequestDto = serde_json::from_str(r#"{"mode":"RepeatAll"}"#).unwrap();
        assert_eq!((a.mode.as_str(), b.mode.as_str()), ("Shuffle", "RepeatAll"));
    }

    #[test]
    fn request_dtos_accept_pascal_and_camel() {
        let new_group: NewGroupRequestDto = serde_json::from_str(r#"{"GroupName":"party"}"#).unwrap();
        assert_eq!(new_group.group_name, "party");
        let join: JoinGroupRequestDto = serde_json::from_str(r#"{"groupId":"abc"}"#).unwrap();
        assert_eq!(join.group_id, "abc");
        let ping: PingRequestDto = serde_json::from_str(r#"{"Ping":42}"#).unwrap();
        assert_eq!(ping.ping, 42);
        let q: SetNewQueueRequestDto =
            serde_json::from_str(r#"{"PlayingQueue":["a","b"],"PlayingItemPosition":1,"StartPositionTicks":5}"#).unwrap();
        assert_eq!(q.playing_queue.len(), 2);
        let s: SeekRequestDto = serde_json::from_str(r#"{"positionTicks":77}"#).unwrap();
        assert_eq!(s.position_ticks, 77);
    }

    #[test]
    fn empty_body_parses_to_default() {
        let req: NewGroupRequestDto = parse_body(&Bytes::from_static(b"  ")).unwrap();
        assert_eq!(req, NewGroupRequestDto::default());
        assert!(parse_body::<NewGroupRequestDto>(&Bytes::from_static(b"{bad")).is_err());
    }

    #[test]
    fn group_info_dto_is_pascal_case() {
        let reg = crate::syncplay::SyncPlayRegistry::default();
        let (g, _) = reg.create("s1", "alice", "night");
        let v = serde_json::to_value(GroupInfoDto::from(&g)).unwrap();
        assert_eq!(v["GroupName"], "night");
        assert_eq!(v["State"], "Idle");
        assert_eq!(v["Participants"], serde_json::json!(["alice"]));
    }

    #[test]
    fn error_mapping_statuses() {
        assert_eq!(map_err(SyncPlayError::GroupNotFound).status_code(), StatusCode::NOT_FOUND);
        assert_eq!(map_err(SyncPlayError::NotInGroup).status_code(), StatusCode::CONFLICT);
        assert_eq!(map_err(SyncPlayError::EmptyQueue).status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
