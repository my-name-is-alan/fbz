-- 转码全局设置（管理端「转码设置」页持久化）。
-- 单行配置表：用固定主键 id=1 + check 约束保证永远只有一行，
-- 读写都走 id=1，upsert 用 on conflict (id)。追加式迁移：仅新建表，对既有 schema 无影响。
--   * hardware_acceleration: 硬件加速后端（none/nvenc/qsv/vaapi/videotoolbox），路由层用 enum 白名单校验。
--   * preferred_encoder:     首选编码器标识（自由文本，路由层限长）。
--   * max_resolution:        转码上限分辨率（480/720/1080/2160/original），路由层用 enum 白名单校验。
--   * segment_duration:      HLS 分片时长（秒），路由层限定 1-60。
--   * throttle:              是否对转码做节流。
create table if not exists transcode_settings (
    id smallint primary key default 1,
    hardware_acceleration text not null default 'none',
    preferred_encoder text not null default 'auto',
    max_resolution text not null default 'original',
    segment_duration integer not null default 6,
    throttle boolean not null default true,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    constraint transcode_settings_singleton check (id = 1)
);
