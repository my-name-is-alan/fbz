//! SyncPlay 进程内组注册表（Jellyfin SyncPlay 最小可用子集）。
//!
//! 组状态保存在进程内（与 [`SessionMessageHub`](crate::realtime::SessionMessageHub)
//! 一样节点本地）；组成员以会话 public id 标识，路由层通过 websocket hub 把
//! `SyncPlayGroupUpdate` / `SyncPlayCommand` 推送给组内所有会话。
//!
//! 已支持：New/Join/Leave/List/Get、SetNewQueue、Pause/Unpause/Seek/Stop、
//! Buffering/Ready（简化：不等待全员 Ready，直接广播）、Ping。
//! 未支持：JF 完整等待全员缓冲的状态机、队列重排/Shuffle/Repeat、跨节点同步。

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::RngCore;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum GroupState {
    Idle,
    Waiting,
    Paused,
    Playing,
}

impl GroupState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Waiting => "Waiting",
            Self::Paused => "Paused",
            Self::Playing => "Playing",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Participant {
    pub session_id: String,
    pub user_name: String,
    pub ping_ms: i64,
}

#[derive(Clone, Debug)]
pub struct PlaylistEntry {
    pub item_id: String,
    pub playlist_item_id: String,
}

#[derive(Clone, Debug)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub state: GroupState,
    pub participants: Vec<Participant>,
    pub playlist: Vec<PlaylistEntry>,
    pub playing_index: i64,
    pub position_ticks: i64,
    pub last_updated_ms: u64,
    /// 正在缓冲的会话；非空时组处于 Waiting，全部 Ready 后才恢复播放。
    pub buffering: HashSet<String>,
    /// Waiting 结束后是否自动继续播放。
    pub resume_after_wait: bool,
    /// 原始（Sorted）顺序；`playlist` 是当前播放顺序（Shuffle 时为打乱后）。
    pub sorted: Vec<PlaylistEntry>,
    pub shuffle: bool,
    pub repeat: RepeatMode,
}

/// JF `GroupRepeatMode`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RepeatMode {
    #[default]
    RepeatNone,
    RepeatAll,
    RepeatOne,
}

impl RepeatMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RepeatNone => "RepeatNone",
            Self::RepeatAll => "RepeatAll",
            Self::RepeatOne => "RepeatOne",
        }
    }
    pub fn parse(v: &str) -> Option<Self> {
        match v {
            "RepeatNone" => Some(Self::RepeatNone),
            "RepeatAll" => Some(Self::RepeatAll),
            "RepeatOne" => Some(Self::RepeatOne),
            _ => None,
        }
    }
}

/// JF `GroupShuffleMode`：`Sorted` / `Shuffle`。
pub fn parse_shuffle_mode(v: &str) -> Option<bool> {
    match v {
        "Sorted" => Some(false),
        "Shuffle" => Some(true),
        _ => None,
    }
}

impl Group {
    pub fn session_ids(&self) -> Vec<String> {
        self.participants.iter().map(|p| p.session_id.clone()).collect()
    }

    pub fn info_json(&self) -> Value {
        json!({
            "GroupId": self.id,
            "GroupName": self.name,
            "State": self.state.as_str(),
            "Participants": self.participants.iter().map(|p| p.user_name.clone()).collect::<Vec<_>>(),
            "LastUpdatedAt": iso8601(self.last_updated_ms),
        })
    }

    pub fn play_queue_json(&self, reason: &str) -> Value {
        json!({
            "Reason": reason,
            "LastUpdate": iso8601(self.last_updated_ms),
            "Playlist": self.playlist.iter().map(|e| json!({
                "ItemId": e.item_id,
                "PlaylistItemId": e.playlist_item_id,
            })).collect::<Vec<_>>(),
            "PlayingItemIndex": self.playing_index,
            "StartPositionTicks": self.position_ticks,
            "IsPlaying": self.state == GroupState::Playing,
            "ShuffleMode": if self.shuffle { "Shuffle" } else { "Sorted" },
            "RepeatMode": self.repeat.as_str(),
        })
    }

    fn current_playlist_item_id(&self) -> String {
        usize::try_from(self.playing_index)
            .ok()
            .and_then(|i| self.playlist.get(i))
            .map(|e| e.playlist_item_id.clone())
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackCommand {
    Pause,
    Unpause,
    Seek(i64),
    Stop,
}

impl PlaybackCommand {
    fn name(self) -> &'static str {
        match self {
            Self::Pause => "Pause",
            Self::Unpause => "Unpause",
            Self::Seek(_) => "Seek",
            Self::Stop => "Stop",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemSwitch {
    Next,
    Previous,
    PlaylistItem(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum SyncPlayError {
    GroupNotFound,
    NotInGroup,
    EmptyQueue,
    ItemNotFound,
}

impl SyncPlayError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::GroupNotFound => "SyncPlay group not found",
            Self::NotInGroup => "session is not in a SyncPlay group",
            Self::EmptyQueue => "SyncPlay queue is empty",
            Self::ItemNotFound => "SyncPlay playlist item not found",
        }
    }
}

/// 一条待推送的出站消息：收件会话 + 已序列化 JSON。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outbound {
    pub session_id: String,
    pub payload: String,
}

#[derive(Default)]
pub struct SyncPlayRegistry {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    groups: HashMap<String, Group>,
    membership: HashMap<String, String>,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

fn new_id() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Unix ms → `YYYY-MM-DDTHH:MM:SS.mmmZ`（无外部时间库依赖）。
pub fn iso8601(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let millis = ms % 1000;
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    // Howard Hinnant civil_from_days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        sod / 3600,
        (sod % 3600) / 60,
        sod % 60
    )
}

fn group_update(group_id: &str, kind: &str, data: Value) -> String {
    json!({
        "MessageType": "SyncPlayGroupUpdate",
        "Data": { "GroupId": group_id, "Type": kind, "Data": data },
    })
    .to_string()
}

fn fanout(sessions: &[String], payload: &str) -> Vec<Outbound> {
    sessions
        .iter()
        .map(|s| Outbound { session_id: s.clone(), payload: payload.to_owned() })
        .collect()
}

impl SyncPlayRegistry {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("syncplay lock poisoned")
    }

    pub fn list(&self) -> Vec<Group> {
        let mut groups: Vec<Group> = self.lock().groups.values().cloned().collect();
        groups.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        groups
    }

    pub fn get(&self, group_id: &str) -> Option<Group> {
        self.lock().groups.get(group_id).cloned()
    }

    pub fn group_of(&self, session_id: &str) -> Option<Group> {
        let inner = self.lock();
        inner.membership.get(session_id).and_then(|g| inner.groups.get(g)).cloned()
    }

    fn leave_locked(inner: &mut Inner, session_id: &str, out: &mut Vec<Outbound>) {
        let Some(group_id) = inner.membership.remove(session_id) else {
            return;
        };
        let mut remove = false;
        if let Some(group) = inner.groups.get_mut(&group_id) {
            let leaving = group
                .participants
                .iter()
                .find(|p| p.session_id == session_id)
                .map(|p| p.user_name.clone())
                .unwrap_or_default();
            group.participants.retain(|p| p.session_id != session_id);
            group.buffering.remove(session_id);
            group.last_updated_ms = now_ms();
            out.push(Outbound {
                session_id: session_id.to_owned(),
                payload: group_update(&group_id, "GroupLeft", json!(group_id)),
            });
            if group.participants.is_empty() {
                remove = true;
            } else {
                let payload = group_update(&group_id, "UserLeft", json!(leaving));
                out.extend(fanout(&group.session_ids(), &payload));
            }
        }
        if remove {
            inner.groups.remove(&group_id);
        }
    }

    /// 新建组并让创建者加入（若已在其它组则先离开）。
    pub fn create(&self, session_id: &str, user_name: &str, name: &str) -> (Group, Vec<Outbound>) {
        let mut inner = self.lock();
        let mut out = Vec::new();
        Self::leave_locked(&mut inner, session_id, &mut out);
        let name = if name.trim().is_empty() { format!("{user_name}'s group") } else { name.trim().to_owned() };
        let group = Group {
            id: new_id(),
            name,
            state: GroupState::Idle,
            participants: vec![Participant {
                session_id: session_id.to_owned(),
                user_name: user_name.to_owned(),
                ping_ms: 0,
            }],
            playlist: Vec::new(),
            playing_index: -1,
            position_ticks: 0,
            last_updated_ms: now_ms(),
            buffering: HashSet::new(),
            resume_after_wait: false,
            sorted: Vec::new(),
            shuffle: false,
            repeat: RepeatMode::RepeatNone,
        };
        inner.membership.insert(session_id.to_owned(), group.id.clone());
        inner.groups.insert(group.id.clone(), group.clone());
        out.push(Outbound {
            session_id: session_id.to_owned(),
            payload: group_update(&group.id, "GroupJoined", group.info_json()),
        });
        (group, out)
    }

    pub fn join(
        &self,
        session_id: &str,
        user_name: &str,
        group_id: &str,
    ) -> Result<(Group, Vec<Outbound>), SyncPlayError> {
        let mut inner = self.lock();
        if !inner.groups.contains_key(group_id) {
            return Err(SyncPlayError::GroupNotFound);
        }
        let mut out = Vec::new();
        if inner.membership.get(session_id).map(String::as_str) != Some(group_id) {
            Self::leave_locked(&mut inner, session_id, &mut out);
        }
        let group = inner.groups.get_mut(group_id).ok_or(SyncPlayError::GroupNotFound)?;
        if !group.participants.iter().any(|p| p.session_id == session_id) {
            let others = group.session_ids();
            group.participants.push(Participant {
                session_id: session_id.to_owned(),
                user_name: user_name.to_owned(),
                ping_ms: 0,
            });
            out.extend(fanout(&others, &group_update(group_id, "UserJoined", json!(user_name))));
        }
        group.last_updated_ms = now_ms();
        let snapshot = group.clone();
        inner.membership.insert(session_id.to_owned(), group_id.to_owned());
        out.push(Outbound {
            session_id: session_id.to_owned(),
            payload: group_update(group_id, "GroupJoined", snapshot.info_json()),
        });
        if !snapshot.playlist.is_empty() {
            out.push(Outbound {
                session_id: session_id.to_owned(),
                payload: group_update(group_id, "PlayQueue", snapshot.play_queue_json("NewPlaylist")),
            });
        }
        Ok((snapshot, out))
    }

    pub fn leave(&self, session_id: &str) -> Result<Vec<Outbound>, SyncPlayError> {
        let mut inner = self.lock();
        if !inner.membership.contains_key(session_id) {
            return Err(SyncPlayError::NotInGroup);
        }
        let mut out = Vec::new();
        Self::leave_locked(&mut inner, session_id, &mut out);
        Ok(out)
    }

    pub fn set_queue(
        &self,
        session_id: &str,
        item_ids: &[String],
        playing_index: i64,
        start_ticks: i64,
    ) -> Result<(Group, Vec<Outbound>), SyncPlayError> {
        if item_ids.is_empty() {
            return Err(SyncPlayError::EmptyQueue);
        }
        let mut inner = self.lock();
        let group_id = inner.membership.get(session_id).cloned().ok_or(SyncPlayError::NotInGroup)?;
        let group = inner.groups.get_mut(&group_id).ok_or(SyncPlayError::GroupNotFound)?;
        group.playlist = item_ids
            .iter()
            .map(|id| PlaylistEntry { item_id: id.clone(), playlist_item_id: new_id() })
            .collect();
        let max = group.playlist.len() as i64 - 1;
        group.playing_index = playing_index.clamp(0, max);
        group.sorted = group.playlist.clone();
        if group.shuffle {
            shuffle_keep_current(group);
        }
        group.position_ticks = start_ticks.max(0);
        group.state = GroupState::Waiting;
        group.buffering = group.participants.iter().map(|p| p.session_id.clone()).collect();
        group.resume_after_wait = true;
        group.last_updated_ms = now_ms();
        let snapshot = group.clone();
        let payload = group_update(&group_id, "PlayQueue", snapshot.play_queue_json("NewPlaylist"));
        let out = fanout(&snapshot.session_ids(), &payload);
        Ok((snapshot, out))
    }

    /// 播放控制：更新组状态并向全员广播 `SyncPlayCommand`。
    pub fn command(
        &self,
        session_id: &str,
        command: PlaybackCommand,
    ) -> Result<(Group, Vec<Outbound>), SyncPlayError> {
        let mut inner = self.lock();
        let group_id = inner.membership.get(session_id).cloned().ok_or(SyncPlayError::NotInGroup)?;
        let group = inner.groups.get_mut(&group_id).ok_or(SyncPlayError::GroupNotFound)?;
        let now = now_ms();
        // 播放中时按墙钟推进位置，保证 Pause 时位置准确。
        if group.state == GroupState::Playing {
            let elapsed = now.saturating_sub(group.last_updated_ms) as i64;
            group.position_ticks += elapsed * 10_000;
        }
        match command {
            PlaybackCommand::Pause => {
                group.state = GroupState::Paused;
                group.resume_after_wait = false;
            }
            PlaybackCommand::Unpause if !group.buffering.is_empty() => {
                // 有成员仍在缓冲：进入 Waiting，全部 Ready 后自动 Unpause。
                group.state = GroupState::Waiting;
                group.resume_after_wait = true;
                group.last_updated_ms = now;
                let snapshot = group.clone();
                let payload = group_update(
                    &group_id,
                    "StateUpdate",
                    json!({ "State": "Waiting", "Reason": "Buffering" }),
                );
                let out = fanout(&snapshot.session_ids(), &payload);
                return Ok((snapshot, out));
            }
            PlaybackCommand::Unpause => group.state = GroupState::Playing,
            PlaybackCommand::Seek(ticks) => {
                group.position_ticks = ticks.max(0);
                if group.state == GroupState::Idle {
                    group.state = GroupState::Paused;
                }
            }
            PlaybackCommand::Stop => {
                group.state = GroupState::Idle;
                group.position_ticks = 0;
                group.buffering.clear();
                group.resume_after_wait = false;
            }
        }
        group.last_updated_ms = now;
        let snapshot = group.clone();
        // 留 500ms 余量让各端同时执行。
        let when = iso8601(now + 500);
        let payload = json!({
            "MessageType": "SyncPlayCommand",
            "Data": {
                "GroupId": group_id,
                "PlaylistItemId": snapshot.current_playlist_item_id(),
                "When": when,
                "PositionTicks": snapshot.position_ticks,
                "Command": command.name(),
                "EmittedAt": iso8601(now),
            },
        })
        .to_string();
        let mut out = fanout(&snapshot.session_ids(), &payload);
        let state_payload = group_update(
            &group_id,
            "StateUpdate",
            json!({ "State": snapshot.state.as_str(), "Reason": command.name() }),
        );
        out.extend(fanout(&snapshot.session_ids(), &state_payload));
        Ok((snapshot, out))
    }

    /// 成员开始缓冲：播放中则全组暂停进入 Waiting（记住需恢复）。
    pub fn buffering(&self, session_id: &str) -> Result<(Group, Vec<Outbound>), SyncPlayError> {
        let was_playing;
        {
            let mut inner = self.lock();
            let group_id = inner.membership.get(session_id).cloned().ok_or(SyncPlayError::NotInGroup)?;
            let group = inner.groups.get_mut(&group_id).ok_or(SyncPlayError::GroupNotFound)?;
            group.buffering.insert(session_id.to_owned());
            was_playing = group.state == GroupState::Playing;
            if !was_playing {
                if group.state != GroupState::Idle {
                    group.state = GroupState::Waiting;
                }
                return Ok((group.clone(), Vec::new()));
            }
        }
        let (mut group, out) = self.command(session_id, PlaybackCommand::Pause)?;
        let mut inner = self.lock();
        if let Some(g) = inner.groups.get_mut(&group.id) {
            g.state = GroupState::Waiting;
            g.resume_after_wait = true;
            group = g.clone();
        }
        Ok((group, out))
    }

    /// 成员缓冲完成；全部就绪且需恢复时广播 Unpause。
    pub fn ready(&self, session_id: &str) -> Result<(Group, Vec<Outbound>), SyncPlayError> {
        let resume;
        {
            let mut inner = self.lock();
            let group_id = inner.membership.get(session_id).cloned().ok_or(SyncPlayError::NotInGroup)?;
            let group = inner.groups.get_mut(&group_id).ok_or(SyncPlayError::GroupNotFound)?;
            group.buffering.remove(session_id);
            if !group.buffering.is_empty() || group.state != GroupState::Waiting {
                return Ok((group.clone(), Vec::new()));
            }
            resume = group.resume_after_wait;
            if !resume {
                group.state = GroupState::Paused;
                return Ok((group.clone(), Vec::new()));
            }
            group.resume_after_wait = false;
        }
        self.command(session_id, PlaybackCommand::Unpause)
    }

    /// 切换当前播放项（NextItem / PreviousItem / SetPlaylistItem）。全员进入缓冲等待。
    pub fn switch_item(&self, session_id: &str, target: ItemSwitch) -> Result<(Group, Vec<Outbound>), SyncPlayError> {
        let mut inner = self.lock();
        let group_id = inner.membership.get(session_id).cloned().ok_or(SyncPlayError::NotInGroup)?;
        let group = inner.groups.get_mut(&group_id).ok_or(SyncPlayError::GroupNotFound)?;
        if group.playlist.is_empty() {
            return Err(SyncPlayError::EmptyQueue);
        }
        let last = group.playlist.len() as i64 - 1;
        let wrap = group.repeat == RepeatMode::RepeatAll;
        let (index, reason) = match target {
            // RepeatOne：Next/Previous 重播当前项（JF PlayQueueManager 语义）。
            ItemSwitch::Next | ItemSwitch::Previous if group.repeat == RepeatMode::RepeatOne => (
                group.playing_index,
                if target == ItemSwitch::Next { "NextItem" } else { "PreviousItem" },
            ),
            ItemSwitch::Next if wrap && group.playing_index >= last => (0, "NextItem"),
            ItemSwitch::Previous if wrap && group.playing_index <= 0 => (last, "PreviousItem"),
            ItemSwitch::Next => (group.playing_index + 1, "NextItem"),
            ItemSwitch::Previous => (group.playing_index - 1, "PreviousItem"),
            ItemSwitch::PlaylistItem(ref id) => (
                group
                    .playlist
                    .iter()
                    .position(|e| &e.playlist_item_id == id)
                    .map(|i| i as i64)
                    .ok_or(SyncPlayError::ItemNotFound)?,
                "SetCurrentItem",
            ),
        };
        if index < 0 || index > last {
            return Err(SyncPlayError::ItemNotFound);
        }
        group.playing_index = index;
        group.position_ticks = 0;
        group.state = GroupState::Waiting;
        group.buffering = group.participants.iter().map(|p| p.session_id.clone()).collect();
        group.resume_after_wait = true;
        group.last_updated_ms = now_ms();
        let snapshot = group.clone();
        let payload = group_update(&group_id, "PlayQueue", snapshot.play_queue_json(reason));
        let out = fanout(&snapshot.session_ids(), &payload);
        Ok((snapshot, out))
    }

    /// `SetShuffleMode`：打乱时当前项移到首位、其余随机；恢复 Sorted 时按原顺序定位当前项。
    pub fn set_shuffle(&self, session_id: &str, shuffle: bool) -> Result<(Group, Vec<Outbound>), SyncPlayError> {
        let mut inner = self.lock();
        let group_id = inner.membership.get(session_id).cloned().ok_or(SyncPlayError::NotInGroup)?;
        let group = inner.groups.get_mut(&group_id).ok_or(SyncPlayError::GroupNotFound)?;
        group.shuffle = shuffle;
        if shuffle {
            shuffle_keep_current(group);
        } else {
            let current = group.current_playlist_item_id();
            group.playlist = group.sorted.clone();
            if let Some(i) = group.playlist.iter().position(|e| e.playlist_item_id == current) {
                group.playing_index = i as i64;
            }
        }
        group.last_updated_ms = now_ms();
        let snapshot = group.clone();
        let payload = group_update(&group_id, "PlayQueue", snapshot.play_queue_json("SetShuffleMode"));
        Ok((snapshot.clone(), fanout(&snapshot.session_ids(), &payload)))
    }

    pub fn set_repeat(&self, session_id: &str, repeat: RepeatMode) -> Result<(Group, Vec<Outbound>), SyncPlayError> {
        let mut inner = self.lock();
        let group_id = inner.membership.get(session_id).cloned().ok_or(SyncPlayError::NotInGroup)?;
        let group = inner.groups.get_mut(&group_id).ok_or(SyncPlayError::GroupNotFound)?;
        group.repeat = repeat;
        group.last_updated_ms = now_ms();
        let snapshot = group.clone();
        let payload = group_update(&group_id, "PlayQueue", snapshot.play_queue_json("SetRepeatMode"));
        Ok((snapshot.clone(), fanout(&snapshot.session_ids(), &payload)))
    }

    /// `Queue`：`next=false` 追加到队尾（Queue）；`next=true` 插到当前项之后（QueueNext）。
    /// 空队列时等同 SetNewQueue。Shuffle 时 sorted 同步插入（原序队尾 / 当前项后）。
    pub fn queue_items(&self, session_id: &str, item_ids: &[String], next: bool) -> Result<(Group, Vec<Outbound>), SyncPlayError> {
        if item_ids.is_empty() {
            return Err(SyncPlayError::EmptyQueue);
        }
        let is_empty = self.group_of(session_id).ok_or(SyncPlayError::NotInGroup)?.playlist.is_empty();
        if is_empty {
            return self.set_queue(session_id, item_ids, 0, 0);
        }
        let mut inner = self.lock();
        let group_id = inner.membership.get(session_id).cloned().ok_or(SyncPlayError::NotInGroup)?;
        let group = inner.groups.get_mut(&group_id).ok_or(SyncPlayError::GroupNotFound)?;
        let entries: Vec<PlaylistEntry> = item_ids
            .iter()
            .map(|id| PlaylistEntry { item_id: id.clone(), playlist_item_id: new_id() })
            .collect();
        let current = group.current_playlist_item_id();
        if next {
            let at = (group.playing_index + 1).clamp(0, group.playlist.len() as i64) as usize;
            group.playlist.splice(at..at, entries.clone());
            let sorted_at = group
                .sorted
                .iter()
                .position(|e| e.playlist_item_id == current)
                .map_or(group.sorted.len(), |i| i + 1);
            group.sorted.splice(sorted_at..sorted_at, entries);
        } else {
            group.playlist.extend(entries.clone());
            group.sorted.extend(entries);
        }
        group.last_updated_ms = now_ms();
        let snapshot = group.clone();
        let reason = if next { "QueueNext" } else { "Queue" };
        let payload = group_update(&group_id, "PlayQueue", snapshot.play_queue_json(reason));
        Ok((snapshot.clone(), fanout(&snapshot.session_ids(), &payload)))
    }

    /// `RemoveFromPlaylist`：按 PlaylistItemId 删除；`clear_playlist` 删除除当前项外全部，
    /// 再加 `clear_playing_item` 连当前项一起清空（组回 Idle）。当前项被删时由原位置的下一项接替
    /// （位置归零、进入 Waiting 等全员就绪）。
    pub fn remove_items(
        &self,
        session_id: &str,
        playlist_item_ids: &[String],
        clear_playlist: bool,
        clear_playing_item: bool,
    ) -> Result<(Group, Vec<Outbound>), SyncPlayError> {
        let mut inner = self.lock();
        let group_id = inner.membership.get(session_id).cloned().ok_or(SyncPlayError::NotInGroup)?;
        let group = inner.groups.get_mut(&group_id).ok_or(SyncPlayError::GroupNotFound)?;
        let current = group.current_playlist_item_id();
        let remove = |e: &PlaylistEntry| {
            if clear_playlist {
                clear_playing_item || e.playlist_item_id != current
            } else {
                playlist_item_ids.iter().any(|id| id == &e.playlist_item_id)
            }
        };
        let before = group.playlist.len();
        // 原当前位置之前保留下来的条目数 = 当前项被删时接替者的新索引。
        let kept_before = group
            .playlist
            .iter()
            .take(group.playing_index.max(0) as usize)
            .filter(|e| !remove(e))
            .count();
        group.playlist.retain(|e| !remove(e));
        group.sorted.retain(|e| !remove(e));
        if group.playlist.len() == before {
            return Err(SyncPlayError::ItemNotFound);
        }
        if let Some(i) = group.playlist.iter().position(|e| e.playlist_item_id == current) {
            group.playing_index = i as i64;
        } else if group.playlist.is_empty() {
            group.playing_index = -1;
            group.position_ticks = 0;
            group.state = GroupState::Idle;
            group.buffering.clear();
        } else {
            group.playing_index = kept_before.min(group.playlist.len() - 1) as i64;
            group.position_ticks = 0;
            group.state = GroupState::Waiting;
            group.buffering = group.participants.iter().map(|p| p.session_id.clone()).collect();
            group.resume_after_wait = true;
        }
        group.last_updated_ms = now_ms();
        let snapshot = group.clone();
        let payload = group_update(&group_id, "PlayQueue", snapshot.play_queue_json("RemoveItems"));
        Ok((snapshot.clone(), fanout(&snapshot.session_ids(), &payload)))
    }

    /// `MovePlaylistItem`：把条目移到 `new_index`（越界夹到队尾），当前项按 id 跟随。
    /// 非 Shuffle 时同步原序（sorted = playlist）。
    pub fn move_item(&self, session_id: &str, playlist_item_id: &str, new_index: i64) -> Result<(Group, Vec<Outbound>), SyncPlayError> {
        let mut inner = self.lock();
        let group_id = inner.membership.get(session_id).cloned().ok_or(SyncPlayError::NotInGroup)?;
        let group = inner.groups.get_mut(&group_id).ok_or(SyncPlayError::GroupNotFound)?;
        let from = group
            .playlist
            .iter()
            .position(|e| e.playlist_item_id == playlist_item_id)
            .ok_or(SyncPlayError::ItemNotFound)?;
        let current = group.current_playlist_item_id();
        let entry = group.playlist.remove(from);
        let to = new_index.clamp(0, group.playlist.len() as i64) as usize;
        group.playlist.insert(to, entry);
        if let Some(i) = group.playlist.iter().position(|e| e.playlist_item_id == current) {
            group.playing_index = i as i64;
        }
        if !group.shuffle {
            group.sorted = group.playlist.clone();
        }
        group.last_updated_ms = now_ms();
        let snapshot = group.clone();
        let payload = group_update(&group_id, "PlayQueue", snapshot.play_queue_json("MoveItem"));
        Ok((snapshot.clone(), fanout(&snapshot.session_ids(), &payload)))
    }

    pub fn ping(&self, session_id: &str, ping_ms: i64) -> Result<(), SyncPlayError> {
        let mut inner = self.lock();
        let group_id = inner.membership.get(session_id).cloned().ok_or(SyncPlayError::NotInGroup)?;
        let group = inner.groups.get_mut(&group_id).ok_or(SyncPlayError::GroupNotFound)?;
        if let Some(p) = group.participants.iter_mut().find(|p| p.session_id == session_id) {
            p.ping_ms = ping_ms.max(0);
        }
        Ok(())
    }
}

fn shuffle_keep_current(group: &mut Group) {
    use rand::seq::SliceRandom;
    let current = usize::try_from(group.playing_index).ok().and_then(|i| group.playlist.get(i).cloned());
    let mut rest: Vec<PlaylistEntry> = group
        .sorted
        .iter()
        .filter(|e| current.as_ref().is_none_or(|c| c.playlist_item_id != e.playlist_item_id))
        .cloned()
        .collect();
    rest.shuffle(&mut rand::thread_rng());
    group.playlist = current.into_iter().chain(rest).collect();
    group.playing_index = if group.playlist.is_empty() { -1 } else { 0 };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_remove_and_move_items() {
        let reg = SyncPlayRegistry::default();
        reg.create("s1", "a", "g");
        let ids = |g: &Group| g.playlist.iter().map(|e| e.item_id.clone()).collect::<Vec<_>>();
        // 空队列 Queue → 新队列。
        let (g, _) = reg.queue_items("s1", &["a".into(), "b".into()], false).unwrap();
        assert_eq!((ids(&g), g.playing_index), (vec!["a".to_owned(), "b".to_owned()], 0));
        let (g, out) = reg.queue_items("s1", &["c".into()], false).unwrap();
        assert_eq!(ids(&g), vec!["a", "b", "c"]);
        assert!(out[0].payload.contains("\"Reason\":\"Queue\""));
        let (g, _) = reg.queue_items("s1", &["n".into()], true).unwrap();
        assert_eq!(ids(&g), vec!["a", "n", "b", "c"]);
        assert_eq!(g.sorted.iter().map(|e| e.item_id.as_str()).collect::<Vec<_>>(), vec!["a", "n", "b", "c"]);
        assert_eq!(reg.queue_items("s1", &[], false).unwrap_err(), SyncPlayError::EmptyQueue);
        // Move：把 c 移到 0，当前项 a 跟随到 1。
        let c = g.playlist[3].playlist_item_id.clone();
        let (g, _) = reg.move_item("s1", &c, 0).unwrap();
        assert_eq!((ids(&g), g.playing_index), (vec!["c".to_owned(), "a".to_owned(), "n".to_owned(), "b".to_owned()], 1));
        assert_eq!(reg.move_item("s1", "nope", 0).unwrap_err(), SyncPlayError::ItemNotFound);
        // Remove 当前项 a → 下一项 n 顶上（索引 1），Waiting。
        let a = g.playlist[1].playlist_item_id.clone();
        let (g, _) = reg.remove_items("s1", &[a], false, false).unwrap();
        assert_eq!((ids(&g), g.playing_index, g.state), (vec!["c".to_owned(), "n".to_owned(), "b".to_owned()], 1, GroupState::Waiting));
        assert_eq!(reg.remove_items("s1", &["nope".into()], false, false).unwrap_err(), SyncPlayError::ItemNotFound);
        // ClearPlaylist 保留当前项；再加 ClearPlayingItem 清空回 Idle。
        let (g, _) = reg.remove_items("s1", &[], true, false).unwrap();
        assert_eq!((ids(&g), g.playing_index), (vec!["n".to_owned()], 0));
        let (g, _) = reg.remove_items("s1", &[], true, true).unwrap();
        assert_eq!((g.playlist.len(), g.playing_index, g.state), (0, -1, GroupState::Idle));
        // 删除队尾的当前项 → 索引夹到新队尾。
        let (g, _) = reg.queue_items("s1", &["x".into(), "y".into()], false).unwrap();
        let y = g.playlist[1].playlist_item_id.clone();
        reg.switch_item("s1", ItemSwitch::PlaylistItem(y.clone())).unwrap();
        let (g, _) = reg.remove_items("s1", &[y], false, false).unwrap();
        assert_eq!((ids(&g), g.playing_index), (vec!["x".to_owned()], 0));
    }

    #[test]
    fn shuffle_and_repeat_modes() {
        let reg = SyncPlayRegistry::default();
        reg.create("s1", "a", "g");
        let ids: Vec<String> = (0..6).map(|i| format!("i{i}")).collect();
        let (g, _) = reg.set_queue("s1", &ids, 2, 0).unwrap();
        let current = g.playlist[2].playlist_item_id.clone();
        let (g, out) = reg.set_shuffle("s1", true).unwrap();
        assert_eq!(types(&out, "s1"), vec!["PlayQueue"]);
        assert_eq!(g.playing_index, 0);
        assert_eq!(g.playlist[0].playlist_item_id, current, "current item stays first");
        let mut seen: Vec<_> = g.playlist.iter().map(|e| e.item_id.clone()).collect();
        seen.sort();
        assert_eq!(seen, ids, "shuffle keeps every item");
        let v: Value = serde_json::from_str(&out[0].payload).unwrap();
        assert_eq!(v["Data"]["Data"]["ShuffleMode"], "Shuffle");
        assert_eq!(v["Data"]["Data"]["Reason"], "SetShuffleMode");
        let (g, _) = reg.set_shuffle("s1", false).unwrap();
        assert_eq!(g.playing_index, 2);
        assert_eq!(g.playlist[2].playlist_item_id, current);

        // RepeatAll wraps at both ends.
        reg.set_repeat("s1", RepeatMode::RepeatAll).unwrap();
        reg.switch_item("s1", ItemSwitch::PlaylistItem(g.playlist[5].playlist_item_id.clone())).unwrap();
        assert_eq!(reg.switch_item("s1", ItemSwitch::Next).unwrap().0.playing_index, 0);
        assert_eq!(reg.switch_item("s1", ItemSwitch::Previous).unwrap().0.playing_index, 5);
        // RepeatOne stays on the current item.
        let (g, out) = reg.set_repeat("s1", RepeatMode::RepeatOne).unwrap();
        let v: Value = serde_json::from_str(&out[0].payload).unwrap();
        assert_eq!(v["Data"]["Data"]["RepeatMode"], "RepeatOne");
        assert_eq!(reg.switch_item("s1", ItemSwitch::Next).unwrap().0.playing_index, g.playing_index);
        // RepeatNone: end of queue is an error again.
        reg.set_repeat("s1", RepeatMode::RepeatNone).unwrap();
        assert_eq!(reg.switch_item("s1", ItemSwitch::Next).unwrap_err(), SyncPlayError::ItemNotFound);
        assert_eq!(RepeatMode::parse("RepeatAll"), Some(RepeatMode::RepeatAll));
        assert_eq!(parse_shuffle_mode("Bogus"), None);
    }

    fn types(out: &[Outbound], session: &str) -> Vec<String> {
        out.iter()
            .filter(|o| o.session_id == session)
            .map(|o| {
                let v: Value = serde_json::from_str(&o.payload).unwrap();
                match v["MessageType"].as_str().unwrap() {
                    "SyncPlayCommand" => format!("Command:{}", v["Data"]["Command"].as_str().unwrap()),
                    _ => v["Data"]["Type"].as_str().unwrap().to_owned(),
                }
            })
            .collect()
    }

    #[test]
    fn iso8601_formats_epoch_and_known_date() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso8601(1_790_000_000_123), "2026-09-21T14:13:20.123Z");
    }

    #[test]
    fn create_join_leave_lifecycle() {
        let reg = SyncPlayRegistry::default();
        let (g, out) = reg.create("s1", "alice", "movie night");
        assert_eq!(types(&out, "s1"), vec!["GroupJoined"]);
        assert_eq!(reg.list().len(), 1);

        let (g2, out) = reg.join("s2", "bob", &g.id).unwrap();
        assert_eq!(g2.participants.len(), 2);
        assert_eq!(types(&out, "s1"), vec!["UserJoined"]);
        assert_eq!(types(&out, "s2"), vec!["GroupJoined"]);

        let out = reg.leave("s1").unwrap();
        assert_eq!(types(&out, "s1"), vec!["GroupLeft"]);
        assert_eq!(types(&out, "s2"), vec!["UserLeft"]);
        assert_eq!(reg.get(&g.id).unwrap().participants.len(), 1);

        reg.leave("s2").unwrap();
        assert!(reg.list().is_empty(), "empty group is removed");
        assert_eq!(reg.leave("s2"), Err(SyncPlayError::NotInGroup));
    }

    #[test]
    fn join_unknown_group_fails_and_create_switches_group() {
        let reg = SyncPlayRegistry::default();
        assert_eq!(reg.join("s1", "a", "nope").unwrap_err(), SyncPlayError::GroupNotFound);
        let (g1, _) = reg.create("s1", "a", "");
        assert_eq!(g1.name, "a's group");
        let (g2, out) = reg.create("s1", "a", "other");
        assert!(reg.get(&g1.id).is_none());
        assert_eq!(reg.group_of("s1").unwrap().id, g2.id);
        assert_eq!(types(&out, "s1"), vec!["GroupLeft", "GroupJoined"]);
    }

    #[test]
    fn playback_commands_broadcast_to_all_members() {
        let reg = SyncPlayRegistry::default();
        let (g, _) = reg.create("s1", "a", "g");
        reg.join("s2", "b", &g.id).unwrap();
        assert_eq!(reg.set_queue("s1", &[], 0, 0).unwrap_err(), SyncPlayError::EmptyQueue);
        let (g, out) = reg.set_queue("s1", &["i1".into(), "i2".into()], 5, 100).unwrap();
        assert_eq!(g.playing_index, 1);
        assert_eq!(types(&out, "s2"), vec!["PlayQueue"]);

        // 新队列后全员缓冲中：Unpause 进入 Waiting；全员 Ready 后自动 Unpause。
        let (g, out) = reg.command("s2", PlaybackCommand::Unpause).unwrap();
        assert_eq!(g.state, GroupState::Waiting);
        assert_eq!(types(&out, "s1"), vec!["StateUpdate"]);
        let (_, out) = reg.ready("s1").unwrap();
        assert!(out.is_empty(), "s2 still buffering");
        let (g, out) = reg.ready("s2").unwrap();
        assert_eq!(g.state, GroupState::Playing);
        assert_eq!(types(&out, "s1"), vec!["Command:Unpause", "StateUpdate"]);

        let (g, out) = reg.command("s1", PlaybackCommand::Seek(9_000)).unwrap();
        assert_eq!(g.position_ticks, 9_000);
        let v: Value = serde_json::from_str(&out[0].payload).unwrap();
        assert_eq!(v["Data"]["PositionTicks"], 9_000);
        assert_eq!(v["Data"]["PlaylistItemId"], g.playlist[1].playlist_item_id.as_str());

        let (g, _) = reg.command("s1", PlaybackCommand::Pause).unwrap();
        assert_eq!(g.state, GroupState::Paused);
        assert!(g.position_ticks >= 9_000);
        let (g, _) = reg.command("s1", PlaybackCommand::Stop).unwrap();
        assert_eq!((g.state, g.position_ticks), (GroupState::Idle, 0));

        assert_eq!(reg.command("s9", PlaybackCommand::Pause).unwrap_err(), SyncPlayError::NotInGroup);
        reg.ping("s1", 40).unwrap();
        assert_eq!(reg.get(&g.id).unwrap().participants[0].ping_ms, 40);
    }

    #[test]
    fn late_joiner_receives_current_queue() {
        let reg = SyncPlayRegistry::default();
        let (g, _) = reg.create("s1", "a", "g");
        reg.set_queue("s1", &["i1".into()], 0, 0).unwrap();
        let (_, out) = reg.join("s2", "b", &g.id).unwrap();
        assert_eq!(types(&out, "s2"), vec!["GroupJoined", "PlayQueue"]);
    }

    #[test]
    fn buffering_member_pauses_group_until_ready() {
        let reg = SyncPlayRegistry::default();
        let (g, _) = reg.create("s1", "a", "g");
        reg.join("s2", "b", &g.id).unwrap();
        reg.set_queue("s1", &["i1".into()], 0, 0).unwrap();
        reg.ready("s1").unwrap();
        let (g, _) = reg.ready("s2").unwrap();
        assert_eq!(g.state, GroupState::Playing);

        let (g, out) = reg.buffering("s2").unwrap();
        assert_eq!(g.state, GroupState::Waiting);
        assert_eq!(types(&out, "s1"), vec!["Command:Pause", "StateUpdate"]);
        let (g, out) = reg.ready("s2").unwrap();
        assert_eq!(g.state, GroupState::Playing);
        assert_eq!(types(&out, "s1"), vec!["Command:Unpause", "StateUpdate"]);

        // 暂停状态下的缓冲不自动恢复播放
        reg.command("s1", PlaybackCommand::Pause).unwrap();
        reg.buffering("s1").unwrap();
        let (g, out) = reg.ready("s1").unwrap();
        assert_eq!(g.state, GroupState::Paused);
        assert!(out.is_empty());
    }

    #[test]
    fn switch_item_moves_index_and_waits() {
        let reg = SyncPlayRegistry::default();
        let (g, _) = reg.create("s1", "a", "g");
        assert_eq!(reg.switch_item("s1", ItemSwitch::Next).unwrap_err(), SyncPlayError::EmptyQueue);
        reg.set_queue("s1", &["e1".into(), "e2".into(), "e3".into()], 0, 0).unwrap();
        let (g2, out) = reg.switch_item("s1", ItemSwitch::Next).unwrap();
        assert_eq!((g2.playing_index, g2.state), (1, GroupState::Waiting));
        let v: Value = serde_json::from_str(&out[0].payload).unwrap();
        assert_eq!(v["Data"]["Data"]["Reason"], "NextItem");
        let target = g2.playlist[2].playlist_item_id.clone();
        assert_eq!(reg.switch_item("s1", ItemSwitch::PlaylistItem(target)).unwrap().0.playing_index, 2);
        assert_eq!(reg.switch_item("s1", ItemSwitch::Next).unwrap_err(), SyncPlayError::ItemNotFound);
        assert_eq!(reg.switch_item("s1", ItemSwitch::Previous).unwrap().0.playing_index, 1);
        assert!(reg.get(&g.id).is_some());
    }
}
