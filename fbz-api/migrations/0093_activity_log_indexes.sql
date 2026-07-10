-- 0093_activity_log_indexes.sql（additive）：System/ActivityLog 聚合的有界探测索引。
-- 活动日志把 登录(sessions) / 任务收尾(jobs) / 播放开始(playback_sessions) 三路
-- 最近事件按时间合并；每路都要 top-N 倒序探测，补齐对应 keyset 索引。

create index if not exists idx_sessions_created_desc
    on sessions (created_at desc, id desc);

create index if not exists idx_jobs_terminal_finished_desc
    on jobs (finished_at desc, id desc)
    where status in ('succeeded', 'failed') and finished_at is not null;

create index if not exists idx_playback_sessions_started_desc
    on playback_sessions (started_at desc, id desc);
