# Webhooks 最小可验（通知目标）

| 字段 | 值 |
|------|-----|
| 日期 | 2026-10-15（PT） |
| 状态 | 最小投递已接线；依赖通知 worker |

## 登记

`POST /api/admin/notification-targets`（管理员）：

```json
{
  "name": "Play started",
  "targetType": "webhook",
  "channel": "playback.started",
  "config": { "url": "https://example.com/hooks/fbz" },
  "isEnabled": true
}
```

`config.url` 也可写成 secretRef 形态（见既有 target_config）。可选 `config.headers`。

## 事件 channel（与 event key 相同）

| channel | 何时发出 |
|---------|----------|
| `playback.started` | 播放开始上报 |
| `playback.stopped` | 播放停止上报 |
| `library.scan.started` | 库扫描开始 |
| `library.scan.completed` | 库扫描成功（含 continuation 块） |
| `library.scan.failed` | 库扫描失败 |

channel 留空的目标**不会**收到上述事件（仅匹配 channel 为空的系统通知，如 Emby `Notifications/Admin`）。

## 投递体

通知 worker 对 webhook 目标 `POST` JSON（camelCase），字段含 `requestId`、`title`、`message`、`level`、`channel`、`metadata`（含 `event` 与播放/扫描上下文）。

## 相关

- Admin CRUD：`/api/admin/notification-targets`
- 投递：`notifications/delivery.rs` + worker
- ScheduledTasks Trigger/Cancel：已有 `POST|DELETE /ScheduledTasks/Running/{taskId}`（及 Emby `…/Delete`）
