-- 0080_photo_metadata.sql（additive，新增表）
-- 图片条目（item_type='photo'，家庭库）的 EXIF / 尺寸 / 缩略图元数据。
-- 与 media_files（视频向：container/duration/bitrate）分离：图片维度不同
-- （拍摄时间 / 相机 / 镜头 / GPS / 朝向），独立 1:1 表更干净。
-- captured_at 索引支撑「时间线」视图；gps 列支撑地图/地点聚合（后续）。
create table if not exists media_photo_metadata (
    media_item_id bigint primary key references media_items(id) on delete cascade,
    width integer,
    height integer,
    captured_at timestamptz,          -- EXIF DateTimeOriginal（无则回退文件 mtime，由上层决定）
    camera_make text,
    camera_model text,
    lens_model text,
    orientation smallint,             -- EXIF Orientation 标准值 1..=8
    iso integer,
    f_number numeric(6, 3),           -- 光圈 f/2.8 → 2.8
    exposure_time text,               -- 快门 "1/250"（分数文本，保真）
    focal_length numeric(7, 2),       -- 焦距 mm
    gps_latitude double precision,
    gps_longitude double precision,
    gps_altitude double precision,
    thumbnail_path text,              -- 生成的缩略图落盘路径（null = 尚未生成）
    extracted_at timestamptz not null default now(),
    check (width is null or width > 0),
    check (height is null or height > 0),
    check (orientation is null or (orientation >= 1 and orientation <= 8)),
    check (iso is null or iso > 0),
    check (f_number is null or f_number > 0),
    check (focal_length is null or focal_length > 0),
    check (gps_latitude is null or (gps_latitude >= -90 and gps_latitude <= 90)),
    check (gps_longitude is null or (gps_longitude >= -180 and gps_longitude <= 180))
);

-- 时间线视图：按拍摄时间倒序翻页（keyset）。
create index if not exists idx_media_photo_metadata_captured_at
    on media_photo_metadata (captured_at desc, media_item_id desc);
