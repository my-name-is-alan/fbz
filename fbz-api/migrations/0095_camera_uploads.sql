-- 相机上传历史（Emby Devices/CameraUploads 兼容）。
-- 每行对应设备成功上传的一个文件；history 查询用于客户端断点续传去重。
create table if not exists camera_uploads (
    id bigserial primary key,
    device_id bigint not null references devices (id) on delete cascade,
    album text not null default '',
    name text not null,
    upload_id text not null default '',
    mime_type text not null default 'application/octet-stream',
    file_path text not null,
    size_bytes bigint not null default 0,
    created_at timestamptz not null default now()
);

-- 客户端以 (album, name, id) 作为文件身份做“是否已上传”判断；同键重传幂等覆盖。
create unique index if not exists uq_camera_uploads_identity
    on camera_uploads (device_id, album, name, upload_id);

create index if not exists idx_camera_uploads_device_created
    on camera_uploads (device_id, created_at desc, id desc);
