-- 0084_media_file_quality_tags.sql（additive，新增列）
-- 阶段 5（media-recognition-design §10）：QualityTags 落库。画质是物理文件属性
-- （同一 media_item 可有多版本 media_file，清晰度各异），故落 media_files 而非 media_items。
-- 扫描时由文件名识别（recognition::QualityTags）填入；probe 跑完后用 ffprobe 实测
-- 分辨率/编码校正文件名标签的冲突（实测优先）。全部 nullable，识别不出即留空。
alter table media_files add column if not exists resolution text;     -- 480p/720p/1080p/2160p
alter table media_files add column if not exists source text;         -- BluRay/WEB-DL/HDTV/Remux
alter table media_files add column if not exists video_codec text;    -- x264/x265/HEVC/AV1
alter table media_files add column if not exists audio_codec text;    -- DTS/AC3/FLAC/AAC
alter table media_files add column if not exists hdr text;            -- HDR/HDR10+/DV
