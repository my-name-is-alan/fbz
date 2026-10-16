<script setup lang="ts">
import type { PlaybackItem, PlaybackEpisode } from "@/stores/playback.ts";
import { loadWebPlayer, withPlaybackDeadline } from "@/service/webplayer.ts";
import type { WebPlayer, EngineInfo, SubtitleController } from "@/service/webplayer.ts";
import { createPlaybackReporter } from "@/service/modules/playback-reporter.ts";
import { useAuthStore } from "@/stores/auth.ts";
import { useSyncPlay } from "@/composables/useSyncPlay.ts";
import type { SyncPlayCommand } from "@/service/modules/syncplay.ts";
import { fetchTrickplay, trickplayFrame } from "@/service/modules/trickplay.ts";
import type { TrickplayInfo } from "@/service/modules/trickplay.ts";
import { resolutionLabel } from "@/service/modules/mediaInfo.ts";
const props = defineProps<{
  item: PlaybackItem;
  playlist: PlaybackEpisode[];
  currentEpisodeIndex: number;
  hasPreviousEpisode: boolean;
  hasNextEpisode: boolean;
}>();
const emit = defineEmits<{
  close: [];
  selectEpisode: [episodeId: string];
  previousEpisode: [];
  nextEpisode: [];
  switchMediaSource: [mediaSourceId: string, resumeSeconds: number];
}>();

/** 根据界面语言 / 字幕偏好拼选轨正则；autoSubtitles=false 时不自动挂轨。 */
function preferredSubtitleRegex(language: string, autoSubtitles: boolean): RegExp | null {
  if (!autoSubtitles) return null;
  const lang = language.toLowerCase();
  if (lang.startsWith("zh-tw") || lang.includes("cht") || lang.includes("hant")) {
    return /zh-tw|zh-hant|cht|繁|traditional/i;
  }
  if (lang.startsWith("zh") || lang.startsWith("chi") || lang === "chs") {
    return /zh|chi|chs|cht|中文|简体|繁体/i;
  }
  if (lang.startsWith("ja")) {
    return /ja|jpn|日本|japanese/i;
  }
  if (lang.startsWith("en")) {
    return /en|eng|english|英文/i;
  }
  const token = lang.split("-")[0] || lang;
  return new RegExp(token.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "i");
}
const video = ref<HTMLVideoElement>();
const surface = ref<HTMLElement>();
const phase = ref<"loading" | "ready" | "error">("loading");
const error = ref("");
const notice = ref("");
const panel = ref<"episodes" | "info" | "syncplay" | null>(null);
/** 底栏弹出菜单（音轨 / 字幕 / 版本 / 倍速）。 */
type PlayerMenu = "audio" | "subtitle" | "version" | "speed";
const menu = ref<PlayerMenu | null>(null);
const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 2];
const playing = ref(false);
const buffering = ref(false);
const visible = ref(true);
const time = ref(0);
const duration = ref(0);
const width = ref(0);
const height = ref(0);
const volume = ref(0.8);
const muted = ref(false);
const speed = ref("1");
const audio = ref("0");
const subtitle = ref("-1");
const info = shallowRef<EngineInfo>();
const fullscreen = ref(false);
const scrubbing = ref(false);
const scrub = ref(0);
/** Trickplay：进度条拖动/悬停预览。 */
const trickplay = shallowRef<TrickplayInfo | null>(null);
const hoverSeconds = ref<number | null>(null);
const previewSeconds = computed(() => (scrubbing.value ? scrub.value : hoverSeconds.value));
const previewFrame = computed(() =>
  trickplay.value && previewSeconds.value !== null
    ? trickplayFrame(props.item.id, trickplay.value, previewSeconds.value)
    : null,
);
const previewLeft = computed(() =>
  duration.value > 0 && previewSeconds.value !== null
    ? `${Math.min(100, Math.max(0, (previewSeconds.value / duration.value) * 100))}%`
    : "0%",
);
function onSeekHover(event: PointerEvent) {
  const el = event.currentTarget as HTMLElement;
  const rect = el.getBoundingClientRect();
  if (!rect.width || !duration.value) return;
  hoverSeconds.value = ((event.clientX - rect.left) / rect.width) * duration.value;
}
watch(
  () => props.item.id,
  async (id) => {
    trickplay.value = null;
    trickplay.value = await fetchTrickplay(id);
  },
  { immediate: true },
);
let engine: WebPlayer | undefined;
let subtitles: SubtitleController | undefined;
/** 当前外挂字幕 <track>（DeliveryUrl）；与内嵌 Subtitles 互斥。 */
let externalTrackEl: HTMLTrackElement | undefined;
let attachSrt: import("@/service/webplayer.ts").AttachSrtFn | undefined;
let reporter: ReturnType<typeof createPlaybackReporter> | undefined;
let serial = 0;
let loadingAbort: AbortController | undefined;
const loadingLabel = ref("正在读取媒体信息");
let hideTimer: ReturnType<typeof setTimeout> | undefined;
let lastReport = 0;
let retries = 0;
let closing = false;
let previousFocus: HTMLElement | null = null;
let appWasInert = false;
onMounted(() => {
  previousFocus = document.activeElement as HTMLElement;
  const app = document.getElementById("app");
  if (app) {
    appWasInert = app.inert;
    app.inert = true;
  }
  surface.value?.focus();
});
const progress = computed(() =>
  duration.value ? Math.min(100, (time.value / duration.value) * 100) : 0,
);
const audioOptions = computed(
  () =>
    info.value?.audio.map((t, i) => ({ label: t.label || `音轨 ${i + 1}`, value: String(i) })) ||
    [],
);
const subtitleOptions = computed(() => {
  const embedded =
    info.value?.subtitles
      .filter((t) => ["ass", "ssa", "pgs", "srt", "vtt"].includes(t.format ?? ""))
      .map((t) => ({
        label: t.label || t.format || "字幕",
        value: String(info.value!.subtitles.indexOf(t)),
      })) || [];
  const external = (props.item.subtitleTracks ?? [])
    .filter((t) => t.isExternal && t.deliveryUrl)
    .map((t) => ({
      label: t.label.startsWith("外挂") ? t.label : `${t.label}（外挂）`,
      value: `ext:${t.id}`,
    }));
  return [{ label: "关闭字幕", value: "-1" }, ...embedded, ...external];
});
const mediaSourceOptions = computed(() =>
  (props.item.mediaSources ?? []).map((s) => ({ label: s.label, value: s.id })),
);
const activeMediaSourceId = computed({
  get: () => props.item.server?.mediaSourceId ?? mediaSourceOptions.value[0]?.value ?? "",
  set: (value: string) => {
    if (!value || value === props.item.server?.mediaSourceId) return;
    emit("switchMediaSource", value, video.value?.currentTime ?? time.value);
  },
});

/** Chapters Intro：IntroStart→IntroEnd 区间内显示「跳过片头」；靠近 CreditsStart 显示「跳过片尾」。 */
const introStart = computed(() => {
  const ch = props.item.chapters?.find((c) => c.markerType === "IntroStart");
  return ch?.startTime;
});
const introEnd = computed(() => {
  const ch = props.item.chapters?.find((c) => c.markerType === "IntroEnd");
  return ch?.startTime;
});
const creditsStart = computed(() => {
  const ch = props.item.chapters?.find((c) => c.markerType === "CreditsStart");
  return ch?.startTime;
});
const showSkipIntro = computed(() => {
  if (phase.value !== "ready") return false;
  const start = introStart.value;
  const end = introEnd.value;
  if (start == null || end == null || end <= start) return false;
  const now = time.value;
  return now >= Math.max(0, start - 0.5) && now < end - 0.75;
});
const showSkipCredits = computed(() => {
  if (phase.value !== "ready") return false;
  const start = creditsStart.value;
  if (start == null) return false;
  const now = time.value;
  const dur = duration.value || 0;
  if (dur > 0 && now >= dur - 1) return false;
  return now >= start - 1 && now < start + 90;
});
function skipIntro() {
  const end = introEnd.value;
  if (end == null) return;
  seek(end);
}
function skipCredits() {
  if (props.hasNextEpisode) {
    goAdjacent(1);
    return;
  }
  const dur = duration.value;
  if (dur > 0) seek(Math.max(0, dur - 0.25));
}
function clearExternalSubtitle() {
  if (externalTrackEl) {
    try {
      externalTrackEl.track.mode = "disabled";
    } catch {
      /* ignore */
    }
    externalTrackEl.remove();
    externalTrackEl = undefined;
  }
}

async function applyExternalSubtitle(trackId: string) {
  const track = (props.item.subtitleTracks ?? []).find((t) => t.id === trackId);
  const element = video.value;
  if (!track?.deliveryUrl || !element || !attachSrt) {
    throw new Error("external subtitle unavailable");
  }
  clearExternalSubtitle();
  await subtitles?.select(-1);
  const response = await fetch(track.deliveryUrl);
  if (!response.ok) throw new Error(`subtitle fetch ${response.status}`);
  const body = await response.text();
  const codec = (track.codec || "").toLowerCase();
  if (codec === "vtt" || codec === "webvtt" || body.trimStart().startsWith("WEBVTT")) {
    const url = URL.createObjectURL(new Blob([body], { type: "text/vtt" }));
    const el = document.createElement("track");
    el.kind = "subtitles";
    el.label = track.label || "外挂";
    el.srclang = track.language || "und";
    el.src = url;
    el.addEventListener("load", () => URL.revokeObjectURL(url), { once: true });
    element.appendChild(el);
    el.track.mode = "showing";
    externalTrackEl = el;
    return;
  }
  externalTrackEl = attachSrt(element, body, {
    label: track.label || "外挂",
    language: track.language || "und",
    asDefault: true,
  });
}

const REPEAT_LABELS: Record<string, string> = {
  RepeatNone: "不循环",
  RepeatAll: "列表循环",
  RepeatOne: "单集循环",
};
/** 播放结束：上报停止；同步观看中由组队列推进（遵循随机/循环模式）。 */
function onVideoEnded() {
  report("stop");
  syncPlay.requestNext();
}

function stamp(value: number) {
  if (!Number.isFinite(value)) return "0:00";
  const seconds = Math.max(0, Math.floor(value));
  return seconds >= 3600
    ? `${Math.floor(seconds / 3600)}:${String(Math.floor(seconds / 60) % 60).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`
    : `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}
async function report(kind: "start" | "progress" | "stop") {
  if (!reporter) return;
  try {
    await reporter.report(
      kind,
      video.value?.currentTime ?? time.value,
      video.value?.paused ?? true,
    );
    if (kind === "stop") window.dispatchEvent(new Event("fbz-playback-saved"));
  } catch {
    notice.value = "观看进度暂未保存，请检查服务器连接。";
  }
}
function reveal() {
  visible.value = true;
  if (hideTimer) clearTimeout(hideTimer);
  hideTimer = setTimeout(() => {
    if (playing.value && !panel.value && !menu.value) visible.value = false;
  }, 3500);
}
async function load(resumeOverride?: number, newReport = true, autoPlay = true) {
  const turn = ++serial;
  loadingAbort?.abort();
  const cancellation = new AbortController();
  loadingAbort = cancellation;
  loadingLabel.value = "正在读取媒体信息";
  phase.value = "loading";
  info.value = undefined;
  width.value = 0;
  height.value = 0;
  time.value = 0;
  duration.value = props.item.duration || 0;
  error.value = "";
  notice.value = "";
  playing.value = false;
  visible.value = true;
  if (newReport) {
    await report("stop");
    const s = props.item.server;
    reporter = s
      ? createPlaybackReporter({
          ItemId: props.item.id,
          MediaSourceId: s.mediaSourceId,
          PlaySessionId: s.playSessionId,
          PlayMethod: "DirectPlay",
        })
      : undefined;
    retries = 0;
  }
  clearExternalSubtitle();
  await subtitles?.destroy();
  subtitles = undefined;
  await engine?.dispose();
  engine = undefined;
  if (turn !== serial) return;
  await nextTick();
  const element = video.value;
  if (!element || !props.item.source?.uri) {
    phase.value = "error";
    error.value = "这部作品暂时没有可播放的媒体源。";
    return;
  }
  try {
    const api = await loadWebPlayer();
    attachSrt = api.attachSrt;
    if (turn !== serial) return;
    const direct = { url: props.item.source.uri, size: props.item.source.size };
    const candidates = props.item.source.proxyUri
      ? [direct, { url: props.item.source.proxyUri, size: props.item.source.size }, direct]
      : [direct];
    let player: WebPlayer | undefined;
    let details: EngineInfo | undefined;
    for (let index = 0; index < candidates.length; index++) {
      if (turn !== serial) return;
      const candidate = new api.Player(element);
      engine = candidate;
      candidate.log = (message, level) => {
        if (
          turn === serial &&
          level === "warn" &&
          /without audio|software audio unavailable/.test(message)
        )
          notice.value = "当前音轨暂不可用，可尝试切换其他音轨。";
      };
      loadingLabel.value = index === 0 ? "正在读取媒体信息" : "正在尝试兼容读取";
      try {
        details = await withPlaybackDeadline(
          candidate.load(candidates[index]!, { allowNative: index === candidates.length - 1 }),
          index === 0 && candidates.length > 1 ? 12000 : 25000,
          cancellation.signal,
        );
        player = candidate;
        break;
      } catch (err) {
        await withPlaybackDeadline(candidate.dispose(), 2000).catch(() => {});
        if (turn !== serial) return;
        if (index === candidates.length - 1) throw err;
      }
    }
    if (!player || !details) throw new Error("PLAYER_UNAVAILABLE");
    player.onStalled = () => {
      if (turn !== serial) return;
      if (retries < 1) {
        retries++;
        void load(element.currentTime, false, playing.value);
      } else {
        phase.value = "error";
        error.value = "视频连接中断，请重试以获取新的播放地址。";
      }
    };
    if (turn !== serial) {
      await player.dispose();
      return;
    }
    info.value = details;
    if (details.native) console.info("webplayer compatibility mode:", details.nativeReason);
    audio.value = "0";
    // 字幕：按 UserConfiguration / 本地偏好选轨；未匹配则默认关闭。
    const authStore = useAuthStore();
    const prefLang = (authStore.language || authStore.audioPreference || "zh").toLowerCase();
    const preferredRe = preferredSubtitleRegex(prefLang, authStore.autoSubtitles);
    const preferredSub = preferredRe
      ? details.subtitles.findIndex(
          (t) =>
            ["ass", "ssa", "pgs", "srt", "vtt"].includes(t.format ?? "") &&
            // webplayer 把语言写进 label（"… [chi]"），EngineTrack 没有独立 language 字段。
            preferredRe.test(`${t.label ?? ""} ${t.format ?? ""}`),
        )
      : -1;
    const preferredExt = preferredRe
      ? (props.item.subtitleTracks ?? []).find(
          (t) =>
            t.isExternal && t.deliveryUrl && preferredRe.test(`${t.label} ${t.language ?? ""}`),
        )
      : undefined;
    if (preferredSub >= 0) subtitle.value = String(preferredSub);
    else if (preferredExt) subtitle.value = `ext:${preferredExt.id}`;
    else subtitle.value = "-1";
    duration.value = details.duration || props.item.duration || 0;
    subtitles = new api.Subtitles(player, { vendor: api.vendor });
    loadingLabel.value = "正在准备音轨与画面";
    await withPlaybackDeadline(
      player.play({ audioIndex: 0, videoIndex: 0 }),
      30000,
      cancellation.signal,
    );
    if (turn !== serial) return;
    if (subtitle.value !== "-1") {
      try {
        if (subtitle.value.startsWith("ext:")) {
          await applyExternalSubtitle(subtitle.value.slice(4));
        } else {
          await subtitles.select(Number(subtitle.value));
        }
      } catch {
        subtitle.value = "-1";
      }
    }
    element.volume = volume.value;
    element.muted = muted.value;
    element.playbackRate = Number(speed.value);
    // startPositionSeconds 显式优先（含 0=从头）；未设时才用服务端续播 ticks。
    const position =
      resumeOverride ??
      (props.item.startPositionSeconds != null
        ? props.item.startPositionSeconds
        : props.item.server
          ? props.item.server.startTicks / 1e7
          : 0);
    if (position > 0) element.currentTime = position;
    phase.value = "ready";
    update();
    try {
      if (autoPlay) await element.play();
    } catch {
      playing.value = false;
    }
    reveal();
  } catch (failure) {
    if (turn === serial) {
      phase.value = "error";
      void engine?.dispose();
      error.value =
        failure instanceof Error && failure.message === "PLAYBACK_TIMEOUT"
          ? "媒体准备超时，请重试或检查网络连接。"
          : "无法播放这个媒体。请检查连接，或尝试其他音轨与兼容设备。";
    }
  }
}
function update() {
  const el = video.value;
  if (!el) return;
  if (Number.isFinite(el.duration)) duration.value = el.duration;
  time.value = el.currentTime;
  width.value = el.videoWidth;
  height.value = el.videoHeight;
  if (phase.value === "ready" && Date.now() - lastReport > 5000) {
    lastReport = Date.now();
    void report("progress");
  }
}
function onPlay() {
  if (phase.value !== "ready") return;
  playing.value = true;
  void report("start");
  reveal();
}
function onPause() {
  playing.value = false;
  visible.value = true;
  if (phase.value === "ready") void report("progress");
}
/** SyncPlay：服务端广播的命令到点后在本端执行（不再回发）。 */
function applySyncCommand(command: SyncPlayCommand) {
  if (!video.value || phase.value !== "ready") return;
  const target = (command.PositionTicks ?? 0) / 1e7;
  if (command.Command === "Seek" || Math.abs(video.value.currentTime - target) > 1) {
    seekLocal(target);
  }
  if (command.Command === "Unpause") void video.value.play().catch(() => undefined);
  else if (command.Command === "Pause" || command.Command === "Stop") video.value.pause();
}
const syncPlay = useSyncPlay({ apply: applySyncCommand });
const syncGroupName = ref("");
/** 同步队列条目标签：能在当前剧集列表里找到就显示季集，否则显示序号。 */
function syncQueueLabel(itemId: string, index: number): string {
  const episode = props.playlist.find((e) => e.id === itemId);
  if (itemId === props.item.id) return `${index + 1}. 正在观看`;
  return episode
    ? `${index + 1}. 第 ${episode.seasonNumber} 季 · 第 ${episode.episodeNumber} 集`
    : `${index + 1}. 第 ${index + 1} 项`;
}
/** 剧集列表里的下一集（不在同步队列中时可加入）。 */
const nextEpisodeForQueue = computed(() => {
  const next = props.playlist[props.currentEpisodeIndex + 1];
  if (!next || syncPlay.state.value.queue.some((q) => q.ItemId === next.id)) return null;
  return next;
});
/** 组内切集走 SyncPlay（全员同步）；不在组内按原逻辑。 */
function goEpisode(id: string) {
  if (!syncPlay.requestItem(id)) emit("selectEpisode", id);
}
function goAdjacent(step: 1 | -1) {
  const target = props.playlist[props.currentEpisodeIndex + step];
  if (target && syncPlay.requestItem(target.id)) return;
  if (step > 0) emit("nextEpisode");
  else emit("previousEpisode");
}
// 组队列切到别的条目（他人切集）→ 本端跟随。
watch(
  () => syncPlay.queueItemId.value,
  (id) => {
    if (id && id !== props.item.id && props.playlist.some((e) => e.id === id)) {
      emit("selectEpisode", id);
    }
  },
);
function openSyncPlay() {
  menu.value = null;
  panel.value = panel.value === "syncplay" ? null : "syncplay";
  if (panel.value === "syncplay") void syncPlay.refresh();
}
function togglePanel(next: "episodes" | "info") {
  menu.value = null;
  panel.value = panel.value === next ? null : next;
  reveal();
}
function toggleMenu(next: PlayerMenu) {
  menu.value = menu.value === next ? null : next;
  if (menu.value) panel.value = null;
  reveal();
}
function pickAudio(value: string) {
  menu.value = null;
  if (audio.value === value) return;
  audio.value = value;
  void changeAudio();
}
function pickSubtitle(value: string) {
  menu.value = null;
  if (subtitle.value === value) return;
  subtitle.value = value;
  void changeSubtitle();
}
function pickVersion(value: string) {
  menu.value = null;
  activeMediaSourceId.value = value;
}
function pickSpeed(value: number) {
  menu.value = null;
  speed.value = String(value);
}
/** 服务端技术信息（PlaybackInfo MediaStreams）；音轨顺序与容器一致，用于补全编码 / 声道。 */
const serverInfo = computed(() => props.item.mediaInfo);
function audioDetail(index: number): string {
  const tracks = info.value?.audio ?? [];
  const server = serverInfo.value?.audio ?? [];
  if (!server.length || server.length !== tracks.length) return "";
  const s = server[index];
  return [s?.codec, s?.layout].filter(Boolean).join(" ");
}
/** 顶栏规格徽章：实际解码分辨率优先，其次服务端流信息；动态范围只认服务端明确标记。 */
const qualityBadges = computed(() => {
  const badges: string[] = [];
  const video = serverInfo.value?.video[0];
  const res = height.value ? resolutionLabel(height.value, width.value) : video?.resolution;
  if (res) badges.push(res);
  if (video?.dynamicRange) badges.push(video.dynamicRange);
  const a = serverInfo.value?.audio.find((x) => x.isDefault) ?? serverInfo.value?.audio[0];
  if (a?.codec) badges.push([a.codec, a.layout].filter(Boolean).join(" "));
  return badges;
});
/** 进度条上的片头 / 片尾标记位置（百分比）。 */
const markerTicks = computed(() => {
  const dur = duration.value;
  if (!dur) return [];
  return (props.item.chapters ?? [])
    .filter(
      (c) => c.markerType && c.markerType !== "Chapter" && c.startTime > 0 && c.startTime < dur,
    )
    .map((c) => ({ id: c.id, left: `${(c.startTime / dur) * 100}%`, title: c.title }));
});
const activeSubtitleLabel = computed(
  () => subtitleOptions.value.find((o) => o.value === subtitle.value)?.label ?? "关闭字幕",
);
async function toggle() {
  if (phase.value !== "ready" || !video.value) return;
  if (!video.value.ended && syncPlay.requestPlayPause(video.value.paused)) {
    reveal();
    return;
  }
  if (video.value.ended) {
    await load(0, true);
    return;
  }
  try {
    if (video.value.paused) await video.value.play();
    else video.value.pause();
  } catch {
    notice.value = "浏览器暂时无法开始播放，请重试。";
  }
  reveal();
}
function seek(value: number) {
  if (phase.value === "ready" && syncPlay.requestSeek(Math.max(0, value))) {
    scrubbing.value = false;
    reveal();
    return;
  }
  seekLocal(value);
}
function seekLocal(value: number) {
  if (!video.value || phase.value !== "ready") return;
  video.value.currentTime = Math.max(0, Math.min(value, duration.value || value));
  time.value = video.value.currentTime;
  scrubbing.value = false;
  reveal();
}
async function changeAudio() {
  if (!engine || !video.value) return;
  const position = video.value.currentTime;
  const wasPlaying = !video.value.paused;
  phase.value = "loading";
  try {
    await subtitles?.select(-1);
    await engine.play({ audioIndex: Number(audio.value) });
    video.value.currentTime = position;
    if (subtitle.value.startsWith("ext:")) await applyExternalSubtitle(subtitle.value.slice(4));
    else if (subtitle.value !== "-1") await subtitles?.select(Number(subtitle.value));
    phase.value = "ready";
    if (wasPlaying) await video.value.play();
  } catch {
    phase.value = "error";
    error.value = "这个音轨暂时无法播放，请重试。";
  }
}
async function changeSubtitle() {
  try {
    if (subtitle.value === "-1") {
      clearExternalSubtitle();
      await subtitles?.select(-1);
      return;
    }
    if (subtitle.value.startsWith("ext:")) {
      await applyExternalSubtitle(subtitle.value.slice(4));
      return;
    }
    clearExternalSubtitle();
    await subtitles?.select(Number(subtitle.value));
  } catch {
    notice.value = "这个字幕轨道暂时无法显示。";
  }
}
async function toggleFullscreen() {
  try {
    if (document.fullscreenElement) await document.exitFullscreen();
    else await surface.value?.requestFullscreen();
  } catch {
    notice.value = "当前设备不支持浏览器全屏，可继续使用页面播放。";
  }
}
function close() {
  if (closing) return;
  closing = true;
  void report("stop");
  emit("close");
}
watch(
  () => [props.item.id, props.item.server?.mediaSourceId, props.item.source?.uri] as const,
  (next, prev) => {
    closing = false;
    // 版本切换时尽量从当前秒数续播（store 已写入 startPositionSeconds）。
    const sameItem = prev && next[0] === prev[0];
    const sourceChanged = !prev || next[1] !== prev[1] || next[2] !== prev[2];
    if (sameItem && sourceChanged && props.item.startPositionSeconds != null) {
      void load(props.item.startPositionSeconds, true);
      return;
    }
    void load();
  },
  { immediate: true },
);
watch(volume, (value) => {
  if (video.value) video.value.volume = value;
});
watch(muted, (value) => {
  if (video.value) video.value.muted = value;
});
watch(speed, (value) => {
  if (video.value) video.value.playbackRate = Number(value);
});
useEventListener(document, "fullscreenchange", () => {
  fullscreen.value = !!document.fullscreenElement;
});
useEventListener(window, "keydown", (event) => {
  if (event.key === "Tab") {
    const controls = Array.from(
      surface.value?.querySelectorAll<HTMLElement>(
        "button:not(:disabled),input:not(:disabled),select:not(:disabled)",
      ) ?? [],
    ).filter((el) => el.offsetParent !== null);
    const first = controls[0],
      last = controls.at(-1);
    if (
      event.shiftKey &&
      (document.activeElement === first || document.activeElement === surface.value)
    ) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
    return;
  }
  if (event.key === "Escape") {
    if (menu.value) menu.value = null;
    else if (panel.value) panel.value = null;
    else void close();
    return;
  }
  if ((event.target as HTMLElement)?.matches("input,select,textarea")) return;
  if (event.code === "Space") {
    event.preventDefault();
    void toggle();
  }
  if (event.key === "ArrowRight") {
    event.preventDefault();
    seek(time.value + 10);
  }
  if (event.key === "ArrowLeft") {
    event.preventDefault();
    seek(time.value - 10);
  }
  if (event.key === "f") void toggleFullscreen();
});
useIntervalFn(() => {
  if (phase.value === "ready" && video.value?.paused) void report("progress");
}, 15000);
onBeforeUnmount(() => {
  syncPlay.dispose();
  const app = document.getElementById("app");
  if (app) app.inert = appWasInert;
  if (previousFocus?.isConnected) previousFocus.focus();
  serial++;
  loadingAbort?.abort();
  phase.value = "loading";
  if (hideTimer) clearTimeout(hideTimer);
  void report("stop");
  clearExternalSubtitle();
  void subtitles?.destroy();
  void engine?.dispose();
});
</script>
<template>
  <section
    ref="surface"
    tabindex="-1"
    class="cinema-player c-force-dark"
    :class="{ 'chrome-hidden': !visible && !panel && !menu && phase === 'ready' }"
    role="dialog"
    aria-label="视频播放器"
    aria-modal="true"
    @pointermove="reveal"
    @pointerdown="reveal"
  >
    <video
      ref="video"
      playsinline
      :poster="item.backdrop || item.poster"
      @play="onPlay"
      @pause="onPause"
      @timeupdate="update"
      @loadedmetadata="update"
      @waiting="
        buffering = true;
        syncPlay.reportBuffering();
      "
      @playing="
        buffering = false;
        syncPlay.reportReady();
      "
      @canplay="syncPlay.reportReady()"
      @ended="onVideoEnded"
      @click="
        menu = null;
        toggle();
      "
    />
    <div class="veil-top" />
    <div class="veil-bottom" />

    <!-- 顶栏 -->
    <header class="player-top">
      <button class="pbtn" type="button" aria-label="关闭播放" @click="close">
        <BaseIcon name="close" :size="22" />
      </button>
      <div class="now-title">
        <h1>{{ item.title }}</h1>
        <span>{{ item.subtitle || "正在观看" }}</span>
      </div>
      <div class="top-badges">
        <span v-for="b in qualityBadges" :key="b" class="q-badge">{{ b }}</span>
        <span
          v-if="mediaSourceOptions.length > 1"
          class="q-badge"
          :title="`${mediaSourceOptions.length} 个版本`"
        >
          <BaseIcon name="versions" :size="11" :stroke-width="2.2" />{{ mediaSourceOptions.length }}
        </span>
      </div>
      <button
        v-if="playlist.length"
        class="pbtn"
        :class="{ on: panel === 'episodes' }"
        type="button"
        aria-label="选集"
        @click="togglePanel('episodes')"
      >
        <BaseIcon name="playlist" :size="20" />
      </button>
    </header>

    <!-- 中央状态 -->
    <div v-if="phase === 'loading'" class="player-state" role="status">
      <BaseIcon name="loading" :size="34" class="c-spin" />
      <h2>{{ loadingLabel }}</h2>
      <p>首次加载音轨和字幕可能需要一点时间。</p>
    </div>
    <div v-else-if="phase === 'error'" class="player-state" role="alert">
      <BaseIcon name="error" :size="34" />
      <h2>暂时无法播放</h2>
      <p>{{ error }}</p>
      <div class="state-actions">
        <button class="du-btn du-btn-primary" type="button" @click="load(time, false)">
          <BaseIcon name="refresh" :size="16" />重新加载
        </button>
        <button class="du-btn du-btn-ghost" type="button" @click="close">返回</button>
      </div>
    </div>
    <div v-else-if="!playing && !panel && !menu" class="center-controls">
      <button class="pbtn round" type="button" aria-label="快退 10 秒" @click="seek(time - 10)">
        <BaseIcon name="rewind" :size="24" /><small>10</small>
      </button>
      <button class="big-play" type="button" aria-label="播放" @click="toggle">
        <BaseIcon name="play" :size="34" fill />
      </button>
      <button class="pbtn round" type="button" aria-label="快进 10 秒" @click="seek(time + 10)">
        <BaseIcon name="forward" :size="24" /><small>10</small>
      </button>
    </div>
    <BaseIcon
      v-if="buffering && playing && phase === 'ready'"
      name="loading"
      :size="40"
      class="c-spin buffer-indicator"
      aria-label="正在缓冲"
    />

    <!-- Skip Intro / Credits（Chapters markers） -->
    <div v-if="showSkipIntro || showSkipCredits" class="skip-markers">
      <button v-if="showSkipIntro" type="button" class="skip-btn" @click="skipIntro">
        <BaseIcon name="next" :size="16" />跳过片头
      </button>
      <button v-if="showSkipCredits" type="button" class="skip-btn" @click="skipCredits">
        <BaseIcon name="next" :size="16" />{{ hasNextEpisode ? "跳过片尾 · 下一集" : "跳过片尾" }}
      </button>
    </div>

    <!-- 侧面板：选集 / 同步观看 / 媒体信息 -->
    <aside v-if="panel" class="player-panel" @pointerdown.stop>
      <header>
        <h2>
          <BaseIcon
            :name="panel === 'episodes' ? 'playlist' : panel === 'syncplay' ? 'users' : 'info'"
            :size="16"
          />
          {{ panel === "episodes" ? "选集" : panel === "syncplay" ? "同步观看" : "媒体信息" }}
        </h2>
        <button class="pbtn sm" type="button" aria-label="关闭面板" @click="panel = null">
          <BaseIcon name="close" :size="16" />
        </button>
      </header>

      <div v-if="panel === 'episodes'" class="episode-queue">
        <button
          v-for="episode in playlist"
          :key="episode.id"
          type="button"
          :class="{ active: episode.id === item.id }"
          @click="goEpisode(episode.id)"
        >
          <div class="queue-image">
            <img
              v-if="episode.backdrop || episode.poster"
              :src="episode.backdrop || episode.poster"
              alt=""
            />
            <BaseIcon v-else name="tv-minimal" :size="22" />
            <span class="ep-no">E{{ String(episode.episodeNumber).padStart(2, "0") }}</span>
          </div>
          <div class="queue-text">
            <strong>{{ episode.subtitle || `第 ${episode.episodeNumber} 集` }}</strong>
            <small>
              S{{ String(episode.seasonNumber).padStart(2, "0") }}E{{
                String(episode.episodeNumber).padStart(2, "0")
              }}
              <template v-if="episode.id === item.id"> · 正在观看</template>
              <template v-else-if="episode.duration">
                · {{ Math.round(episode.duration / 60) }} 分钟</template
              >
            </small>
          </div>
          <BaseIcon v-if="episode.id === item.id" name="audio" :size="16" class="now" />
        </button>
      </div>

      <div v-else-if="panel === 'syncplay'" class="syncplay-panel">
        <template v-if="syncPlay.state.value.group">
          <div class="sync-card">
            <strong
              ><BaseIcon name="users" :size="15" />{{
                syncPlay.state.value.group.GroupName
              }}</strong
            >
            <span class="sync-state">{{ syncPlay.state.value.group.State }}</span>
            <p>成员：{{ syncPlay.state.value.group.Participants.join("、") }}</p>
          </div>
          <div class="sync-row">
            <button
              class="chip-btn"
              :class="{ on: syncPlay.state.value.shuffleMode === 'Shuffle' }"
              type="button"
              :disabled="syncPlay.busy.value || syncPlay.state.value.queue.length < 2"
              @click="syncPlay.toggleShuffle()"
            >
              <BaseIcon name="shuffle" :size="15" />随机：{{
                syncPlay.state.value.shuffleMode === "Shuffle" ? "开" : "关"
              }}
            </button>
            <button
              class="chip-btn"
              :class="{ on: syncPlay.state.value.repeatMode !== 'RepeatNone' }"
              type="button"
              :disabled="syncPlay.busy.value"
              @click="syncPlay.cycleRepeat()"
            >
              <BaseIcon name="repeat" :size="15" />{{
                REPEAT_LABELS[syncPlay.state.value.repeatMode]
              }}
            </button>
          </div>
          <template v-if="syncPlay.state.value.queue.length">
            <p class="panel-label">同步队列</p>
            <div
              v-for="(entry, index) in syncPlay.state.value.queue"
              :key="entry.PlaylistItemId"
              class="sync-queue-row"
            >
              <span :class="{ 'is-current': index === syncPlay.state.value.playingIndex }">
                {{ syncQueueLabel(entry.ItemId, index) }}
              </span>
              <button
                class="pbtn sm"
                type="button"
                aria-label="上移"
                :disabled="syncPlay.busy.value || index === 0"
                @click="syncPlay.moveQueueItem(entry.PlaylistItemId, index - 1)"
              >
                <BaseIcon name="up" :size="15" />
              </button>
              <button
                class="pbtn sm"
                type="button"
                aria-label="移除"
                :disabled="syncPlay.busy.value"
                @click="syncPlay.removeQueueItem(entry.PlaylistItemId)"
              >
                <BaseIcon name="close" :size="15" />
              </button>
            </div>
          </template>
          <button
            v-if="nextEpisodeForQueue"
            class="chip-btn wide"
            type="button"
            :disabled="syncPlay.busy.value"
            @click="syncPlay.queueNext(nextEpisodeForQueue.id)"
          >
            <BaseIcon name="playlist-add" :size="15" />下一集加入同步队列
          </button>
          <button
            class="chip-btn wide danger"
            type="button"
            :disabled="syncPlay.busy.value"
            @click="syncPlay.leave()"
          >
            <BaseIcon name="logout" :size="15" />退出同步观看
          </button>
        </template>
        <template v-else>
          <label class="field">
            <span class="panel-label">新建群组</span>
            <input
              v-model="syncGroupName"
              class="du-input du-input-sm"
              placeholder="群组名称（可选）"
            />
          </label>
          <button
            class="du-btn du-btn-primary du-btn-sm"
            type="button"
            :disabled="syncPlay.busy.value"
            @click="
              syncPlay.create(
                syncGroupName,
                item.id,
                time,
                playlist.map((e) => e.id),
              )
            "
          >
            <BaseIcon name="plus" :size="15" />创建并同步当前视频
          </button>
          <p class="panel-label">可加入的群组</p>
          <p v-if="!syncPlay.groups.value.length" class="panel-note">暂无可加入的群组。</p>
          <button
            v-for="group in syncPlay.groups.value"
            :key="group.GroupId"
            class="chip-btn wide"
            type="button"
            :disabled="syncPlay.busy.value"
            @click="syncPlay.join(group.GroupId)"
          >
            <BaseIcon name="users" :size="15" />加入 {{ group.GroupName }}（{{
              group.Participants.length
            }}
            人）
          </button>
        </template>
        <p v-if="syncPlay.error.value" class="panel-note error">{{ syncPlay.error.value }}</p>
      </div>

      <div v-else class="info-panel">
        <dl class="engine-info">
          <div>
            <dt>播放器</dt>
            <dd>webplayer</dd>
          </div>
          <div>
            <dt>播放方式</dt>
            <dd>{{ info?.native ? "浏览器原生播放" : "浏览器内重封装" }}</dd>
          </div>
          <div>
            <dt>解码画面</dt>
            <dd>{{ width && height ? `${width} × ${height}` : "—" }}</dd>
          </div>
          <div>
            <dt>时长</dt>
            <dd>{{ stamp(duration) }}</dd>
          </div>
          <div>
            <dt>当前音轨</dt>
            <dd>{{ audioOptions[Number(audio)]?.label || "—" }}</dd>
          </div>
          <div>
            <dt>当前字幕</dt>
            <dd>{{ activeSubtitleLabel }}</dd>
          </div>
          <div v-if="mediaSourceOptions.length">
            <dt>版本</dt>
            <dd>
              {{ mediaSourceOptions.find((s) => s.value === activeMediaSourceId)?.label || "—" }}
            </dd>
          </div>
        </dl>
        <MediaInfoPanel v-if="serverInfo" :sources="[serverInfo]" compact />
        <p v-else class="panel-note">服务端未返回该版本的流信息。</p>
      </div>
    </aside>

    <!-- 底栏 -->
    <footer class="player-bottom" @pointerdown.stop>
      <p v-if="notice" class="player-notice" role="status">
        <BaseIcon name="info" :size="15" />{{ notice }}
      </p>

      <!-- 弹出菜单 -->
      <div v-if="menu" class="menu-pop" role="menu" :aria-label="menu">
        <template v-if="menu === 'audio'">
          <p class="menu-title"><BaseIcon name="audio" :size="14" />音轨</p>
          <button
            v-for="opt in audioOptions"
            :key="opt.value"
            type="button"
            role="menuitemradio"
            class="menu-item"
            :aria-checked="audio === opt.value"
            @click="pickAudio(opt.value)"
          >
            <BaseIcon name="check" :size="15" class="tick" :class="{ on: audio === opt.value }" />
            <span class="mi-label">{{ opt.label }}</span>
            <small v-if="audioDetail(Number(opt.value))">{{
              audioDetail(Number(opt.value))
            }}</small>
          </button>
          <p v-if="!audioOptions.length" class="menu-empty">没有可切换的音轨</p>
        </template>
        <template v-else-if="menu === 'subtitle'">
          <p class="menu-title"><BaseIcon name="subtitles" :size="14" />字幕</p>
          <button
            v-for="opt in subtitleOptions"
            :key="opt.value"
            type="button"
            role="menuitemradio"
            class="menu-item"
            :aria-checked="subtitle === opt.value"
            @click="pickSubtitle(opt.value)"
          >
            <BaseIcon
              name="check"
              :size="15"
              class="tick"
              :class="{ on: subtitle === opt.value }"
            />
            <span class="mi-label">{{ opt.label }}</span>
            <small v-if="opt.value.startsWith('ext:')" class="ext">外挂</small>
            <small v-else-if="opt.value !== '-1'">内嵌</small>
          </button>
          <p v-if="subtitleOptions.length <= 1" class="menu-empty">当前媒体没有可切换的字幕</p>
        </template>
        <template v-else-if="menu === 'version'">
          <p class="menu-title"><BaseIcon name="versions" :size="14" />版本</p>
          <button
            v-for="opt in mediaSourceOptions"
            :key="opt.value"
            type="button"
            role="menuitemradio"
            class="menu-item"
            :aria-checked="activeMediaSourceId === opt.value"
            @click="pickVersion(opt.value)"
          >
            <BaseIcon
              name="check"
              :size="15"
              class="tick"
              :class="{ on: activeMediaSourceId === opt.value }"
            />
            <span class="mi-label">{{ opt.label }}</span>
          </button>
        </template>
        <template v-else>
          <p class="menu-title"><BaseIcon name="speed" :size="14" />播放速度</p>
          <button
            v-for="s in SPEEDS"
            :key="s"
            type="button"
            role="menuitemradio"
            class="menu-item"
            :aria-checked="Number(speed) === s"
            @click="pickSpeed(s)"
          >
            <BaseIcon name="check" :size="15" class="tick" :class="{ on: Number(speed) === s }" />
            <span class="mi-label">{{ s === 1 ? "正常" : `${s}×` }}</span>
          </button>
        </template>
      </div>

      <div class="control-panel">
        <div class="seek-row">
          <span class="t">{{ stamp(scrubbing ? scrub : time) }}</span>
          <div class="seek-wrap" :style="{ '--range-progress': `${progress}%` }">
            <div
              v-if="previewFrame"
              class="trickplay-preview"
              :style="{
                left: previewLeft,
                width: `${previewFrame.width}px`,
                height: `${previewFrame.height}px`,
                backgroundImage: `url(${previewFrame.url})`,
                backgroundPosition: `${previewFrame.offsetX}px ${previewFrame.offsetY}px`,
                backgroundSize: `${previewFrame.sheetWidth}px ${previewFrame.sheetHeight}px`,
              }"
            >
              <span>{{ stamp(previewSeconds ?? 0) }}</span>
            </div>
            <span
              v-for="tick in markerTicks"
              :key="tick.id"
              class="marker-tick"
              :style="{ left: tick.left }"
              :title="tick.title"
            />
            <input
              class="seek"
              type="range"
              min="0"
              :max="duration || 1"
              step=".1"
              :value="scrubbing ? scrub : time"
              :disabled="phase !== 'ready'"
              aria-label="播放进度"
              @pointermove="onSeekHover"
              @pointerleave="hoverSeconds = null"
              @input="
                scrubbing = true;
                scrub = Number(($event.target as HTMLInputElement).value);
              "
              @change="seek(Number(($event.target as HTMLInputElement).value))"
            />
          </div>
          <span class="t dim"
            >-{{ stamp(Math.max(0, duration - (scrubbing ? scrub : time))) }}</span
          >
        </div>

        <div class="control-row">
          <div class="left-controls">
            <button
              class="play-main"
              type="button"
              :aria-label="playing ? '暂停' : '播放'"
              :disabled="phase !== 'ready'"
              @click="toggle"
            >
              <BaseIcon :name="playing ? 'pause' : 'play'" :size="22" :fill="true" />
            </button>
            <button
              v-if="playlist.length"
              class="pbtn"
              type="button"
              aria-label="上一集"
              :disabled="!hasPreviousEpisode"
              @click="goAdjacent(-1)"
            >
              <BaseIcon name="previous" :size="20" />
            </button>
            <button
              class="pbtn hide-sm"
              type="button"
              aria-label="快退 10 秒"
              :disabled="phase !== 'ready'"
              @click="seek(time - 10)"
            >
              <BaseIcon name="rewind" :size="19" />
            </button>
            <button
              class="pbtn hide-sm"
              type="button"
              aria-label="快进 10 秒"
              :disabled="phase !== 'ready'"
              @click="seek(time + 10)"
            >
              <BaseIcon name="forward" :size="19" />
            </button>
            <button
              v-if="playlist.length"
              class="pbtn"
              type="button"
              aria-label="下一集"
              :disabled="!hasNextEpisode"
              @click="goAdjacent(1)"
            >
              <BaseIcon name="next" :size="20" />
            </button>
            <div class="volume-control">
              <button
                class="pbtn"
                type="button"
                :aria-label="muted ? '取消静音' : '静音'"
                @click="muted = !muted"
              >
                <BaseIcon :name="muted || !volume ? 'mute' : 'volume'" :size="20" />
              </button>
              <input
                v-model.number="volume"
                class="vol"
                type="range"
                min="0"
                max="1"
                step=".02"
                aria-label="音量"
                :style="{ '--range-progress': `${muted ? 0 : volume * 100}%` }"
              />
            </div>
          </div>

          <div class="right-controls">
            <button
              class="pbtn"
              :class="{ on: syncPlay.active.value || panel === 'syncplay' }"
              type="button"
              aria-label="同步观看"
              title="同步观看"
              @click="openSyncPlay"
            >
              <BaseIcon name="users" :size="19" />
            </button>
            <button
              class="pbtn"
              :class="{ on: menu === 'audio' }"
              type="button"
              aria-label="音轨"
              title="音轨"
              :disabled="!audioOptions.length"
              @click="toggleMenu('audio')"
            >
              <BaseIcon name="audio" :size="19" />
            </button>
            <button
              class="pbtn"
              :class="{ on: menu === 'subtitle', lit: subtitle !== '-1' }"
              type="button"
              aria-label="字幕"
              title="字幕"
              @click="toggleMenu('subtitle')"
            >
              <BaseIcon name="subtitles" :size="19" />
            </button>
            <button
              v-if="mediaSourceOptions.length > 1"
              class="pbtn"
              :class="{ on: menu === 'version' }"
              type="button"
              aria-label="版本"
              title="版本"
              @click="toggleMenu('version')"
            >
              <BaseIcon name="versions" :size="19" />
            </button>
            <button
              class="pbtn speed"
              :class="{ on: menu === 'speed', lit: speed !== '1' }"
              type="button"
              aria-label="播放速度"
              title="播放速度"
              @click="toggleMenu('speed')"
            >
              <span v-if="speed !== '1'">{{ speed }}×</span>
              <BaseIcon v-else name="speed" :size="19" />
            </button>
            <button
              class="pbtn"
              :class="{ on: panel === 'info' }"
              type="button"
              aria-label="媒体信息"
              title="媒体信息"
              @click="togglePanel('info')"
            >
              <BaseIcon name="info" :size="19" />
            </button>
            <button
              class="pbtn"
              type="button"
              :aria-label="fullscreen ? '退出全屏' : '进入全屏'"
              @click="toggleFullscreen"
            >
              <BaseIcon :name="fullscreen ? 'minimize' : 'fullscreen'" :size="19" />
            </button>
          </div>
        </div>
      </div>
    </footer>
  </section>
</template>

<style scoped lang="scss">
.cinema-player {
  position: fixed;
  inset: 0;
  z-index: 100;
  overflow: hidden;
  background: #000;
  outline: none;
  user-select: none;

  video {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    background: #000;
  }

  // 控件统一淡入淡出；隐藏时不再拦截指针，视频可直接点击。
  .veil-top,
  .veil-bottom,
  .player-top,
  .player-bottom,
  .skip-markers {
    transition: opacity 260ms var(--ui-ease-out);
  }

  &.chrome-hidden {
    cursor: none;

    .veil-top,
    .veil-bottom,
    .player-top,
    .player-bottom {
      opacity: 0;
      pointer-events: none;
    }
  }
}

.veil-top,
.veil-bottom {
  position: absolute;
  left: 0;
  right: 0;
  pointer-events: none;
}

.veil-top {
  top: 0;
  height: 160px;
  background: linear-gradient(180deg, rgb(0 0 0 / 72%), transparent);
}

.veil-bottom {
  bottom: 0;
  height: 260px;
  background: linear-gradient(0deg, rgb(2 3 8 / 92%), rgb(2 3 8 / 50%) 55%, transparent);
}

/* ---------- 通用按钮 ---------- */
.pbtn {
  position: relative;
  min-width: 40px;
  height: 40px;
  display: inline-grid;
  grid-auto-flow: column;
  place-items: center;
  gap: 2px;
  padding: 0 8px;
  border: 1px solid transparent;
  border-radius: var(--ui-radius-md);
  background: transparent;
  color: var(--ui-text);
  cursor: pointer;
  transition:
    background var(--ui-duration-fast),
    color var(--ui-duration-fast),
    border-color var(--ui-duration-fast);

  &:hover:not(:disabled) {
    background: rgb(255 255 255 / 10%);
  }

  &:disabled {
    opacity: 0.35;
    cursor: default;
  }

  &.on {
    border-color: rgb(var(--ui-accent-channel) / 55%);
    background: rgb(var(--ui-accent-channel) / 16%);
    color: var(--ui-accent-hover);
  }

  &.lit {
    color: var(--ui-accent-hover);
  }

  &.sm {
    min-width: 30px;
    height: 30px;
    padding: 0 6px;
    border-radius: var(--ui-radius-sm);
  }

  &.round {
    width: 52px;
    height: 52px;
    border-radius: 50%;
    background: rgb(0 0 0 / 35%);
    backdrop-filter: blur(8px);

    small {
      position: absolute;
      bottom: 9px;
      font-size: 9px;
      font-weight: 700;
    }
  }

  &.speed span {
    font-family: var(--ui-font-mono);
    font-size: 12px;
    font-weight: 700;
  }
}

/* ---------- 顶栏 ---------- */
.player-top {
  position: absolute;
  inset: 0 0 auto 0;
  z-index: 3;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 18px 24px;
}

.now-title {
  min-width: 0;
  display: flex;
  flex-direction: column;

  h1 {
    margin: 0;
    overflow: hidden;
    color: #fff;
    font-size: 16px;
    font-weight: 600;
    white-space: nowrap;
    text-overflow: ellipsis;
    text-shadow: 0 1px 6px rgb(0 0 0 / 60%);
  }

  span {
    color: var(--ui-text-muted);
    font-size: 12px;
  }
}

.top-badges {
  display: flex;
  gap: 6px;
  margin-left: auto;
}

.q-badge {
  height: 20px;
  padding: 0 6px;
  display: inline-flex;
  align-items: center;
  gap: 3px;
  border: 1px solid rgb(255 255 255 / 22%);
  border-radius: 4px;
  background: rgb(0 0 0 / 35%);
  color: #fff;
  font-size: 10.5px;
  font-weight: 700;
}

/* ---------- 中央 ---------- */
.player-state {
  position: absolute;
  inset: 0;
  z-index: 2;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 24px;
  color: #fff;
  text-align: center;
  background: rgb(2 3 8 / 60%);

  h2 {
    margin: 8px 0 0;
    font-size: 17px;
    font-weight: 600;
  }

  p {
    max-width: 440px;
    margin: 0;
    color: var(--ui-text-muted);
    font-size: 13px;
  }
}

.state-actions {
  display: flex;
  gap: 8px;
  margin-top: 12px;
}

.center-controls {
  position: absolute;
  top: 50%;
  left: 50%;
  z-index: 2;
  display: flex;
  align-items: center;
  gap: 28px;
  transform: translate(-50%, -50%);
}

.big-play {
  width: 76px;
  height: 76px;
  display: grid;
  place-items: center;
  padding-left: 4px;
  border: 0;
  border-radius: var(--ui-radius-lg);
  background: var(--ui-accent);
  color: var(--ui-accent-contrast);
  box-shadow: var(--ui-shadow-accent);
  cursor: pointer;

  &:hover {
    background: var(--ui-accent-hover);
  }
}

.buffer-indicator {
  position: absolute;
  top: 50%;
  left: 50%;
  z-index: 2;
  margin: -20px 0 0 -20px;
  color: #fff;
}

/* ---------- Skip ---------- */
.skip-markers {
  position: absolute;
  right: 32px;
  bottom: 168px;
  z-index: 4;
  display: flex;
  gap: 8px;
}

.skip-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 40px;
  padding: 0 16px;
  border: 1px solid rgb(255 255 255 / 30%);
  border-radius: var(--ui-radius-md);
  background: var(--ui-glass-strong);
  color: #fff;
  font-size: 13.5px;
  font-weight: 600;
  backdrop-filter: blur(12px);
  cursor: pointer;

  &:hover {
    border-color: var(--ui-accent);
    color: var(--ui-accent-hover);
  }
}

/* ---------- 底栏 ---------- */
.player-bottom {
  position: absolute;
  inset: auto 0 0 0;
  z-index: 4;
  padding: 0 24px 24px;
}

.player-notice {
  display: flex;
  align-items: center;
  gap: 6px;
  width: fit-content;
  max-width: 100%;
  margin: 0 auto 10px;
  padding: 8px 12px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-glass-strong);
  color: var(--ui-text);
  font-size: 12.5px;
}

.control-panel {
  max-width: 1280px;
  margin: 0 auto;
  padding: 14px 16px 10px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  background: var(--ui-glass);
  box-shadow: var(--ui-shadow-dialog);
  backdrop-filter: blur(20px);
}

.seek-row {
  display: flex;
  align-items: center;
  gap: 12px;

  .t {
    flex: 0 0 auto;
    min-width: 52px;
    color: var(--ui-text);
    font-family: var(--ui-font-mono);
    font-size: 12px;
    text-align: right;

    &.dim {
      color: var(--ui-text-subtle);
      text-align: left;
    }
  }
}

.seek-wrap {
  position: relative;
  flex: 1;
  height: 20px;
  display: flex;
  align-items: center;
}

.marker-tick {
  position: absolute;
  top: 50%;
  z-index: 1;
  width: 3px;
  height: 10px;
  margin: -5px 0 0 -1px;
  border-radius: 2px;
  background: var(--ui-accent-hover);
  pointer-events: none;
}

input[type="range"] {
  width: 100%;
  margin: 0;
  appearance: none;
  background: transparent;
  cursor: pointer;

  &::-webkit-slider-runnable-track {
    height: 5px;
    border-radius: 99px;
    background: linear-gradient(
      90deg,
      var(--ui-accent) 0 var(--range-progress, 0%),
      rgb(255 255 255 / 18%) var(--range-progress, 0%) 100%
    );
  }

  &::-moz-range-track {
    height: 5px;
    border-radius: 99px;
    background: rgb(255 255 255 / 18%);
  }

  &::-moz-range-progress {
    height: 5px;
    border-radius: 99px;
    background: var(--ui-accent);
  }

  &::-webkit-slider-thumb {
    width: 14px;
    height: 14px;
    margin-top: -4.5px;
    appearance: none;
    border: 0;
    border-radius: 50%;
    background: var(--ui-accent-hover);
    box-shadow: 0 0 10px rgb(var(--ui-accent-channel) / 60%);
    opacity: 0;
    transition: opacity var(--ui-duration-fast);
  }

  &::-moz-range-thumb {
    width: 14px;
    height: 14px;
    border: 0;
    border-radius: 50%;
    background: var(--ui-accent-hover);
  }

  &:hover::-webkit-slider-thumb,
  &:focus-visible::-webkit-slider-thumb {
    opacity: 1;
  }

  &:disabled {
    cursor: default;
  }
}

.seek-wrap input[type="range"] {
  --range-progress: inherit;
}

.trickplay-preview {
  position: absolute;
  bottom: 22px;
  z-index: 3;
  overflow: hidden;
  border: 2px solid rgb(255 255 255 / 85%);
  border-radius: var(--ui-radius-sm);
  background-repeat: no-repeat;
  box-shadow: var(--ui-shadow-float);
  transform: translateX(-50%);
  pointer-events: none;

  span {
    position: absolute;
    left: 50%;
    bottom: 4px;
    padding: 1px 6px;
    border-radius: 4px;
    background: rgb(0 0 0 / 72%);
    color: #fff;
    font-family: var(--ui-font-mono);
    font-size: 11px;
    transform: translateX(-50%);
  }
}

.control-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-top: 8px;
}

.left-controls,
.right-controls {
  display: flex;
  align-items: center;
  gap: 4px;
}

.play-main {
  width: 46px;
  height: 46px;
  display: grid;
  place-items: center;
  margin-right: 6px;
  border: 0;
  border-radius: var(--ui-radius-md);
  background: var(--ui-accent);
  color: var(--ui-accent-contrast);
  box-shadow: var(--ui-shadow-accent);
  cursor: pointer;

  &:hover:not(:disabled) {
    background: var(--ui-accent-hover);
  }

  &:disabled {
    opacity: 0.5;
  }
}

.volume-control {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-left: 6px;

  .vol {
    width: 90px;
  }
}

/* ---------- 弹出菜单 ---------- */
.menu-pop {
  position: absolute;
  right: max(24px, calc((100% - 1280px) / 2));
  bottom: calc(100% - 14px);
  z-index: 5;
  width: 300px;
  max-height: min(60vh, 420px);
  overflow-y: auto;
  padding: 6px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-glass-strong);
  box-shadow: var(--ui-shadow-float);
  backdrop-filter: blur(20px);
}

.menu-title {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0;
  padding: 8px 10px 6px;
  color: var(--ui-text-subtle);
  font-size: 11.5px;
  font-weight: 600;
}

.menu-item {
  width: 100%;
  min-height: 36px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 10px;
  border: 0;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  color: var(--ui-text);
  font-size: 13px;
  text-align: left;
  cursor: pointer;

  &:hover {
    background: var(--ui-surface-hover);
  }

  &[aria-checked="true"] {
    color: var(--ui-accent-hover);
  }

  .tick {
    opacity: 0;

    &.on {
      opacity: 1;
    }
  }

  .mi-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  small {
    flex: 0 0 auto;
    color: var(--ui-text-subtle);
    font-family: var(--ui-font-mono);
    font-size: 11px;

    &.ext {
      padding: 0 5px;
      border: 1px solid rgb(var(--ui-accent-channel) / 50%);
      border-radius: 4px;
      color: var(--ui-accent-hover);
      font-family: var(--ui-font-sans);
    }
  }
}

.menu-empty {
  margin: 0;
  padding: 10px;
  color: var(--ui-text-subtle);
  font-size: 12.5px;
}

/* ---------- 侧面板 ---------- */
.player-panel {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  z-index: 6;
  width: min(400px, 100%);
  display: flex;
  flex-direction: column;
  border-left: 1px solid var(--ui-border);
  background: var(--ui-glass-strong);
  backdrop-filter: blur(24px);
  box-shadow: var(--ui-shadow-dialog);

  > header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 16px 12px;
    border-bottom: 1px solid var(--ui-border);

    h2 {
      display: flex;
      align-items: center;
      gap: 8px;
      margin: 0;
      color: var(--ui-text);
      font-size: 15px;
      font-weight: 600;
    }
  }

  > div {
    flex: 1;
    overflow-y: auto;
    padding: 12px 16px 24px;
  }
}

.episode-queue {
  display: flex;
  flex-direction: column;
  gap: 4px;

  button {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px;
    border: 1px solid transparent;
    border-radius: var(--ui-radius-md);
    background: transparent;
    color: var(--ui-text);
    text-align: left;
    cursor: pointer;

    &:hover {
      background: var(--ui-surface-hover);
    }

    &.active {
      border-color: rgb(var(--ui-accent-channel) / 45%);
      background: rgb(var(--ui-accent-channel) / 10%);
    }
  }

  .queue-image {
    position: relative;
    flex: 0 0 auto;
    width: 120px;
    aspect-ratio: 16 / 9;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: var(--ui-radius-sm);
    background: var(--ui-surface-raised);
    color: var(--ui-text-subtle);

    img {
      width: 100%;
      height: 100%;
      object-fit: cover;
    }
  }

  .ep-no {
    position: absolute;
    left: 4px;
    bottom: 4px;
    padding: 0 4px;
    border-radius: 3px;
    background: rgb(0 0 0 / 70%);
    color: #fff;
    font-family: var(--ui-font-mono);
    font-size: 10px;
  }

  .queue-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;

    strong {
      overflow: hidden;
      font-size: 13px;
      font-weight: 500;
      white-space: nowrap;
      text-overflow: ellipsis;
    }

    small {
      color: var(--ui-text-subtle);
      font-size: 11.5px;
    }
  }

  .now {
    color: var(--ui-accent-hover);
  }
}

.syncplay-panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.sync-card {
  padding: 12px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-surface);

  strong {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 14px;
  }

  p {
    margin: 6px 0 0;
    color: var(--ui-text-muted);
    font-size: 12.5px;
  }
}

.sync-state {
  display: inline-block;
  margin-top: 4px;
  color: var(--ui-accent-hover);
  font-size: 12px;
}

.sync-row {
  display: flex;
  gap: 8px;
}

.chip-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: 34px;
  padding: 0 12px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-surface);
  color: var(--ui-text);
  font-size: 12.5px;
  cursor: pointer;

  &:hover:not(:disabled) {
    border-color: var(--ui-border-strong);
    background: var(--ui-surface-hover);
  }

  &:disabled {
    opacity: 0.45;
  }

  &.on {
    border-color: rgb(var(--ui-accent-channel) / 55%);
    color: var(--ui-accent-hover);
  }

  &.wide {
    width: 100%;
  }

  &.danger:hover:not(:disabled) {
    color: var(--ui-danger);
  }
}

.sync-queue-row {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12.5px;

  span {
    flex: 1;
    color: var(--ui-text-muted);

    &.is-current {
      color: var(--ui-accent-hover);
      font-weight: 600;
    }
  }
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.panel-label {
  margin: 6px 0 0;
  color: var(--ui-text-subtle);
  font-size: 11.5px;
  font-weight: 600;
}

.panel-note {
  margin: 0;
  color: var(--ui-text-muted);
  font-size: 12.5px;

  &.error {
    color: var(--ui-danger);
  }
}

.info-panel {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.engine-info {
  display: grid;
  gap: 6px;
  margin: 0;
  padding: 12px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-surface);

  div {
    display: flex;
    gap: 10px;
    font-size: 12.5px;
  }

  dt {
    flex: 0 0 72px;
    color: var(--ui-text-subtle);
  }

  dd {
    margin: 0;
    color: var(--ui-text);
  }
}

.info-panel :deep(.summary) {
  grid-template-columns: repeat(2, 1fr);
}

.info-panel :deep(.sum.caps) {
  grid-column: span 2;
}

@media (max-width: 760px) {
  .player-top {
    padding: 12px;
  }

  .top-badges {
    display: none;
  }

  .player-bottom {
    padding: 0 8px 8px;
  }

  .control-panel {
    padding: 10px;
  }

  .volume-control .vol,
  .hide-sm {
    display: none;
  }

  .skip-markers {
    right: 12px;
    bottom: 150px;
  }

  .menu-pop {
    right: 8px;
    left: 8px;
    width: auto;
  }
}
</style>
