//! 进程内会话实时通道注册表。
//!
//! Emby 客户端通过 `/embywebsocket` 建立 websocket 后，按会话 public id 注册
//! 一条出站消息通道；远程控制入口（`Sessions/{Id}/Playing`、`Command`、
//! `Message` 等）把 Emby 兼容消息投递到目标会话的通道。
//!
//! 注册表是节点本地的：websocket 连接天然绑定接入节点，多 API 节点部署时
//! 需要负载均衡把同一设备的 REST 与 websocket 流量粘到同一节点（与 Emby
//! 单进程模型一致）。跨节点指令分发可在后续接 Redis pub/sub 时扩展。

use std::collections::HashMap;
use std::sync::Mutex;

use tokio::sync::mpsc;

#[derive(Default)]
pub struct SessionMessageHub {
    inner: Mutex<HubInner>,
}

#[derive(Default)]
struct HubInner {
    next_generation: u64,
    channels: HashMap<String, HubChannel>,
}

struct HubChannel {
    generation: u64,
    sender: mpsc::UnboundedSender<String>,
}

impl SessionMessageHub {
    /// 为会话注册一条新的出站通道；同一会话重复连接时，新连接替换旧连接
    /// （旧连接的 receiver 会因 sender 被丢弃而收到 None，随后自行退出）。
    /// 返回本次注册的 generation，用于断开时只清理属于自己的注册。
    pub fn register(&self, session_id: &str) -> (u64, mpsc::UnboundedReceiver<String>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut inner = self.inner.lock().expect("session hub lock poisoned");
        inner.next_generation += 1;
        let generation = inner.next_generation;
        inner.channels.insert(
            session_id.to_owned(),
            HubChannel { generation, sender },
        );
        (generation, receiver)
    }

    /// 注销会话通道。仅当当前注册仍是本连接的 generation 时移除，避免
    /// 旧连接的清理误删新连接的注册。
    pub fn unregister(&self, session_id: &str, generation: u64) {
        let mut inner = self.inner.lock().expect("session hub lock poisoned");
        if inner
            .channels
            .get(session_id)
            .is_some_and(|channel| channel.generation == generation)
        {
            inner.channels.remove(session_id);
        }
    }

    /// 把一条已序列化的消息投递给目标会话。返回是否投递成功
    /// （会话未连接或通道已关闭视为失败）。
    pub fn send_to_session(&self, session_id: &str, message: String) -> bool {
        let mut inner = self.inner.lock().expect("session hub lock poisoned");
        let Some(channel) = inner.channels.get(session_id) else {
            return false;
        };
        if channel.sender.send(message).is_ok() {
            return true;
        }
        // 通道已死（连接任务退出但尚未注销），顺手清理。
        inner.channels.remove(session_id);
        false
    }

    /// 目标会话当前是否有活跃 websocket 连接。
    pub fn is_connected(&self, session_id: &str) -> bool {
        let inner = self.inner.lock().expect("session hub lock poisoned");
        inner
            .channels
            .get(session_id)
            .is_some_and(|channel| !channel.sender.is_closed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn send_to_registered_session_delivers_message() {
        let hub = SessionMessageHub::default();
        let (_generation, mut receiver) = hub.register("session-1");

        assert!(hub.send_to_session("session-1", "hello".to_owned()));
        assert_eq!(receiver.recv().await.as_deref(), Some("hello"));
    }

    #[test]
    fn send_to_unknown_session_reports_not_connected() {
        let hub = SessionMessageHub::default();

        assert!(!hub.send_to_session("session-missing", "hello".to_owned()));
        assert!(!hub.is_connected("session-missing"));
    }

    #[test]
    fn new_connection_replaces_previous_registration() {
        let hub = SessionMessageHub::default();
        let (old_generation, old_receiver) = hub.register("session-1");
        let (_new_generation, _new_receiver) = hub.register("session-1");

        // 旧连接退出时的清理不能移除新连接的注册。
        drop(old_receiver);
        hub.unregister("session-1", old_generation);

        assert!(hub.is_connected("session-1"));
    }

    #[test]
    fn dropped_receiver_marks_session_disconnected() {
        let hub = SessionMessageHub::default();
        let (_generation, receiver) = hub.register("session-1");
        drop(receiver);

        assert!(!hub.send_to_session("session-1", "hello".to_owned()));
        assert!(!hub.is_connected("session-1"));
    }
}
