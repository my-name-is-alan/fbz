-- 用户头像元数据。头像二进制存磁盘（artwork_cache_dir/avatars/<public_id>），
-- 这里只记录 content-type 与更新时间：
--   * avatar_content_type 非空即表示该用户已设置自定义头像（否则前端回退首字母头像）。
--   * avatar_updated_at 供前端做缓存击穿（?v=<epoch>），头像更换后 URL 立即变化。
-- 追加式迁移：仅新增可空列，对既有行无影响。
alter table users
    add column if not exists avatar_content_type text,
    add column if not exists avatar_updated_at timestamptz;
