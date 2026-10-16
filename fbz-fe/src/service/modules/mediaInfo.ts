/**
 * 媒体技术信息（A/V 元数据）视图模型。
 *
 * 字段一律来自 Emby 兼容 DTO（`fbz-api/src/compat/emby/dto/mod.rs` 的 `MediaSourceDto` /
 * `MediaStreamDto`），PascalCase 入站、camelCase 出站；后端没给的字段保持 undefined，
 * UI 隐藏对应行——不猜测、不造假。HDR / 杜比视界只在流的 Profile / 标题 / Codec 明确
 * 写出时才标注；仅有 10bit 时只显示「10bit」。
 */

/** `MediaStreamDto`（只列 C 端会读的字段）。 */
export interface MediaStreamDto {
  Index?: number | null;
  Type?: string | null;
  Codec?: string | null;
  CodecTag?: string | null;
  Language?: string | null;
  Title?: string | null;
  DisplayTitle?: string | null;
  Profile?: string | null;
  Level?: number | null;
  Width?: number | null;
  Height?: number | null;
  Channels?: number | null;
  SampleRate?: number | null;
  BitDepth?: number | null;
  BitRate?: number | null;
  IsDefault?: boolean | null;
  IsForced?: boolean | null;
  IsExternal?: boolean | null;
  DeliveryMethod?: string | null;
  DeliveryUrl?: string | null;
}

/** `MediaSourceDto`（只列 C 端会读的字段）。 */
export interface MediaSourceDto {
  Id?: string | null;
  Name?: string | null;
  Container?: string | null;
  RunTimeTicks?: number | null;
  Size?: number | null;
  Bitrate?: number | null;
  IsRemote?: boolean | null;
  Protocol?: string | null;
  MediaStreams?: MediaStreamDto[] | null;
  DefaultAudioStreamIndex?: number | null;
  DefaultSubtitleStreamIndex?: number | null;
  SupportsDirectPlay?: boolean | null;
  SupportsDirectStream?: boolean | null;
  SupportsTranscoding?: boolean | null;
}

export interface VideoStreamInfo {
  index?: number;
  codec?: string;
  profile?: string;
  level?: number;
  width?: number;
  height?: number;
  /** 4K / 1080P …（由 Height/Width 推导）。 */
  resolution?: string;
  bitDepth?: number;
  bitRate?: number;
  /** 仅当 DTO 明确写出时：HDR10 / HDR10+ / HLG / Dolby Vision。 */
  dynamicRange?: string;
  title?: string;
}

export interface AudioStreamInfo {
  index?: number;
  codec?: string;
  profile?: string;
  channels?: number;
  /** 2.0 / 5.1 / 7.1 … */
  layout?: string;
  sampleRate?: number;
  bitRate?: number;
  language?: string;
  title?: string;
  isDefault: boolean;
  isForced: boolean;
  isExternal: boolean;
}

export interface SubtitleStreamInfo {
  index?: number;
  codec?: string;
  language?: string;
  title?: string;
  isDefault: boolean;
  isForced: boolean;
  isExternal: boolean;
  deliveryMethod?: string;
  deliveryUrl?: string;
}

export interface MediaSourceInfo {
  id: string;
  /** 版本名（多版本时用于切换）。 */
  name: string;
  container?: string;
  runtimeSeconds?: number;
  size?: number;
  bitrate?: number;
  isRemote: boolean;
  supportsDirectPlay?: boolean;
  supportsDirectStream?: boolean;
  supportsTranscoding?: boolean;
  defaultAudioIndex?: number;
  defaultSubtitleIndex?: number;
  video: VideoStreamInfo[];
  audio: AudioStreamInfo[];
  subtitles: SubtitleStreamInfo[];
}

function opt<T>(value: T | null | undefined): T | undefined {
  return value == null ? undefined : value;
}

function text(value: string | null | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

/** Emby 容器名规范化（与后端 `matroska` → `mkv` 口径一致）。 */
export function normalizeContainer(container: string | null | undefined): string | undefined {
  const value = text(container)?.toLowerCase();
  if (!value) return undefined;
  const first = value.split(",")[0]!.trim();
  if (first === "matroska" || first === "webm,matroska") return "mkv";
  if (first === "mov" && value.includes("mp4")) return "mp4";
  return first;
}

/** 分辨率标签：按高度为主，宽度兜底（宽银幕 3840×1600 仍算 4K）。 */
export function resolutionLabel(
  height: number | null | undefined,
  width?: number | null,
): string | undefined {
  const h = height ?? 0;
  const w = width ?? 0;
  if (!h && !w) return undefined;
  if (h >= 2000 || w >= 3800) return "4K";
  if (h >= 1400 || w >= 2500) return "2K";
  if (h >= 1000 || w >= 1900) return "1080P";
  if (h >= 700 || w >= 1260) return "720P";
  if (h >= 560) return "576P";
  if (h >= 470) return "480P";
  return h ? `${h}P` : undefined;
}

/** 声道数 → 布局文案。 */
export function channelLayout(channels: number | null | undefined): string | undefined {
  switch (channels) {
    case null:
    case undefined:
    case 0:
      return undefined;
    case 1:
      return "1.0";
    case 2:
      return "2.0";
    case 3:
      return "2.1";
    case 6:
      return "5.1";
    case 7:
      return "6.1";
    case 8:
      return "7.1";
    default:
      return `${channels}ch`;
  }
}

/**
 * 动态范围：只认 DTO 里明确出现的标记（Profile / Title / DisplayTitle / Codec / CodecTag）。
 * `dvhe`/`dvh1`/`dav1` 是杜比视界的码流标记；不从 BitDepth 推断 HDR。
 */
export function dynamicRangeOf(stream: MediaStreamDto): string | undefined {
  const haystack = [
    stream.Profile,
    stream.Title,
    stream.DisplayTitle,
    stream.Codec,
    stream.CodecTag,
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
  if (!haystack) return undefined;
  if (/dolby\s*vision|\bdovi\b|\bdvhe\b|\bdvh1\b|\bdav1\b/.test(haystack)) return "Dolby Vision";
  if (/hdr10\+|hdr10plus/.test(haystack)) return "HDR10+";
  if (/hdr10/.test(haystack)) return "HDR10";
  if (/\bhlg\b/.test(haystack)) return "HLG";
  if (/\bhdr\b/.test(haystack)) return "HDR";
  return undefined;
}

function isType(stream: MediaStreamDto, type: string): boolean {
  return (stream.Type ?? "").toLowerCase() === type;
}

function toVideo(stream: MediaStreamDto): VideoStreamInfo {
  return {
    index: opt(stream.Index),
    codec: text(stream.Codec)?.toUpperCase(),
    profile: text(stream.Profile),
    level: opt(stream.Level),
    width: opt(stream.Width),
    height: opt(stream.Height),
    resolution: resolutionLabel(stream.Height, stream.Width),
    bitDepth: opt(stream.BitDepth),
    bitRate: opt(stream.BitRate),
    dynamicRange: dynamicRangeOf(stream),
    title: text(stream.DisplayTitle) ?? text(stream.Title),
  };
}

function toAudio(stream: MediaStreamDto): AudioStreamInfo {
  return {
    index: opt(stream.Index),
    codec: text(stream.Codec)?.toUpperCase(),
    profile: text(stream.Profile),
    channels: opt(stream.Channels),
    layout: channelLayout(stream.Channels),
    sampleRate: opt(stream.SampleRate),
    bitRate: opt(stream.BitRate),
    language: text(stream.Language),
    title: text(stream.DisplayTitle) ?? text(stream.Title),
    isDefault: Boolean(stream.IsDefault),
    isForced: Boolean(stream.IsForced),
    isExternal: Boolean(stream.IsExternal),
  };
}

function toSubtitle(stream: MediaStreamDto): SubtitleStreamInfo {
  return {
    index: opt(stream.Index),
    codec: text(stream.Codec)?.toUpperCase(),
    language: text(stream.Language),
    title: text(stream.DisplayTitle) ?? text(stream.Title),
    isDefault: Boolean(stream.IsDefault),
    isForced: Boolean(stream.IsForced),
    isExternal: Boolean(stream.IsExternal) || stream.DeliveryMethod === "External",
    deliveryMethod: text(stream.DeliveryMethod),
    deliveryUrl: text(stream.DeliveryUrl),
  };
}

/** 单个 `MediaSourceDto` → 视图模型。 */
export function toMediaSourceInfo(source: MediaSourceDto, index = 0): MediaSourceInfo {
  const streams = source.MediaStreams ?? [];
  const ticks = source.RunTimeTicks ?? 0;
  return {
    id: text(source.Id) ?? `source-${index}`,
    name: text(source.Name) ?? `版本 ${index + 1}`,
    container: normalizeContainer(source.Container),
    runtimeSeconds: ticks > 0 ? Math.round(ticks / 10_000_000) : undefined,
    size: opt(source.Size),
    bitrate: opt(source.Bitrate),
    isRemote: Boolean(source.IsRemote),
    supportsDirectPlay: opt(source.SupportsDirectPlay),
    supportsDirectStream: opt(source.SupportsDirectStream),
    supportsTranscoding: opt(source.SupportsTranscoding),
    defaultAudioIndex: opt(source.DefaultAudioStreamIndex),
    defaultSubtitleIndex: opt(source.DefaultSubtitleStreamIndex),
    video: streams.filter((s) => isType(s, "video")).map(toVideo),
    audio: streams.filter((s) => isType(s, "audio")).map(toAudio),
    subtitles: streams.filter((s) => isType(s, "subtitle")).map(toSubtitle),
  };
}

/** 主音轨：默认标记优先，否则 DefaultAudioStreamIndex，否则第一条。 */
export function primaryAudio(info: MediaSourceInfo): AudioStreamInfo | undefined {
  return (
    info.audio.find((a) => a.isDefault) ??
    info.audio.find((a) => a.index != null && a.index === info.defaultAudioIndex) ??
    info.audio[0]
  );
}

/**
 * 规格徽章（复合状态）：分辨率 · 动态范围 / 位深 · 视频编码 · 主音轨 · 容器。
 * 每项都来自真实流字段；没有就不出。
 */
export function sourceBadges(info: MediaSourceInfo): string[] {
  const badges: string[] = [];
  const video = info.video[0];
  if (video?.resolution) badges.push(video.resolution);
  if (video?.dynamicRange) badges.push(video.dynamicRange);
  else if (video?.bitDepth && video.bitDepth >= 10) badges.push(`${video.bitDepth}bit`);
  if (video?.codec) badges.push(video.codec);
  const audio = primaryAudio(info);
  if (audio?.codec) badges.push(audio.layout ? `${audio.codec} ${audio.layout}` : audio.codec);
  if (info.container) badges.push(info.container.toUpperCase());
  return badges;
}

/** 多版本切换文案：版本名 + 分辨率 + 容器，信息不足时退化为「版本 N」。 */
export function sourceLabel(info: MediaSourceInfo, index: number): string {
  const video = info.video[0];
  const parts = [video?.resolution, video?.dynamicRange, info.container?.toUpperCase()].filter(
    Boolean,
  );
  // 后端摘要源常以文件 id / uuid 作 Name，这类内部标识不展示。
  const name = info.name && !/^[0-9a-f-]{8,}$|^\d+$/i.test(info.name) ? info.name : "";
  if (name && parts.length) return `${name} · ${parts.join(" · ")}`;
  if (parts.length) return parts.join(" · ");
  return name || `版本 ${index + 1}`;
}

const SIZE_UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(bytes: number | null | undefined): string | undefined {
  if (!bytes || bytes <= 0) return undefined;
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < SIZE_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 100 || unit === 0 ? Math.round(value) : value.toFixed(value >= 10 ? 1 : 2)} ${SIZE_UNITS[unit]}`;
}

export function formatBitrate(bitsPerSecond: number | null | undefined): string | undefined {
  if (!bitsPerSecond || bitsPerSecond <= 0) return undefined;
  if (bitsPerSecond >= 1_000_000) return `${(bitsPerSecond / 1_000_000).toFixed(1)} Mbps`;
  return `${Math.round(bitsPerSecond / 1000)} kbps`;
}

export function formatSampleRate(hz: number | null | undefined): string | undefined {
  if (!hz || hz <= 0) return undefined;
  return `${(hz / 1000).toFixed(hz % 1000 === 0 ? 0 : 1)} kHz`;
}

/** 秒 → `2:06:41` / `48:03`。 */
export function formatClock(seconds: number | null | undefined): string | undefined {
  if (!seconds || seconds <= 0) return undefined;
  const total = Math.floor(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return h
    ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
    : `${m}:${String(s).padStart(2, "0")}`;
}
