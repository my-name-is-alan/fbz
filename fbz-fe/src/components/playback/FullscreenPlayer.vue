<script setup lang="ts">
import type { PlaybackItem, PlaybackEpisode } from "@/stores/playback.ts";
import { loadWebPlayer, withPlaybackDeadline } from "@/service/webplayer.ts";
import type { WebPlayer, EngineInfo, SubtitleController } from "@/service/webplayer.ts";
import { createPlaybackReporter } from "@/service/modules/playback-reporter.ts";
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
}>();
const video = ref<HTMLVideoElement>();
const surface = ref<HTMLElement>();
const phase = ref<"loading" | "ready" | "error">("loading");
const error = ref("");
const notice = ref("");
const panel = ref<"episodes" | "settings" | "info" | null>(null);
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
let engine: WebPlayer | undefined;
let subtitles: SubtitleController | undefined;
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
const subtitleOptions = computed(() => [
  { label: "关闭字幕", value: "-1" },
  ...(info.value?.subtitles
    .filter((t) => ["ass", "ssa", "pgs", "srt", "vtt"].includes(t.format ?? ""))
    .map((t) => ({
      label: t.label || t.format || "字幕",
      value: String(info.value!.subtitles.indexOf(t)),
    })) || []),
]);
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
    if (playing.value && !panel.value) visible.value = false;
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
    subtitle.value = "-1";
    duration.value = details.duration || props.item.duration || 0;
    subtitles = new api.Subtitles(player, { vendor: api.vendor });
    loadingLabel.value = "正在准备音轨与画面";
    await withPlaybackDeadline(
      player.play({ audioIndex: 0, videoIndex: 0 }),
      30000,
      cancellation.signal,
    );
    if (turn !== serial) return;
    element.volume = volume.value;
    element.muted = muted.value;
    element.playbackRate = Number(speed.value);
    const position = resumeOverride ?? (props.item.server?.startTicks ?? 0) / 1e7;
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
async function toggle() {
  if (phase.value !== "ready" || !video.value) return;
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
    if (subtitle.value !== "-1") await subtitles?.select(Number(subtitle.value));
    phase.value = "ready";
    if (wasPlaying) await video.value.play();
  } catch {
    phase.value = "error";
    error.value = "这个音轨暂时无法播放，请重试。";
  }
}
async function changeSubtitle() {
  try {
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
  () => props.item.id,
  () => {
    closing = false;
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
    if (panel.value) panel.value = null;
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
  const app = document.getElementById("app");
  if (app) app.inert = appWasInert;
  if (previousFocus?.isConnected) previousFocus.focus();
  serial++;
  loadingAbort?.abort();
  phase.value = "loading";
  if (hideTimer) clearTimeout(hideTimer);
  void report("stop");
  void subtitles?.destroy();
  void engine?.dispose();
});
</script>
<template>
  <section
    ref="surface"
    tabindex="-1"
    class="cinema-player"
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
      @waiting="buffering = true"
      @playing="buffering = false"
      @ended="report('stop')"
      @click="toggle"
    />
    <div class="player-veil" :class="{ hidden: !visible && !panel && phase === 'ready' }" />
    <header class="player-top" :class="{ hidden: !visible && !panel && phase === 'ready' }">
      <button class="du-btn du-btn-ghost du-btn-circle" aria-label="关闭播放" @click="close">
        <BaseIcon name="left" :size="23" />
      </button>
      <div class="now-title">
        <span>{{ item.subtitle || "正在观看" }}</span>
        <h1>{{ item.title }}</h1>
      </div>
      <span v-if="height" class="quality-pill">{{ height }}p</span
      ><button
        v-if="playlist.length"
        class="du-btn du-btn-ghost du-btn-circle"
        aria-label="打开选集"
        @click="panel = panel === 'episodes' ? null : 'episodes'"
      >
        <BaseIcon name="list" :size="22" />
      </button>
    </header>
    <div v-if="phase === 'loading'" class="player-state" role="status">
      <span class="du-loading du-loading-spinner du-loading-lg" />
      <h2>{{ loadingLabel }}</h2>
      <p>首次加载音轨和字幕可能需要一点时间。</p>
    </div>
    <div v-else-if="phase === 'error'" class="player-state" role="alert">
      <BaseIcon name="error" :size="36" />
      <h2>暂时无法播放</h2>
      <p>{{ error }}</p>
      <div>
        <button class="du-btn retry-button" @click="load(time, false)">
          <BaseIcon name="refresh" :size="17" />重新加载</button
        ><button class="du-btn du-btn-ghost" @click="close">返回媒体库</button>
      </div>
    </div>
    <div v-else-if="!playing && !panel" class="center-controls">
      <button
        class="du-btn du-btn-ghost du-btn-circle skip-button"
        aria-label="快退 10 秒"
        @click="seek(time - 10)"
      >
        <BaseIcon name="rewind" :size="25" /><small>10</small></button
      ><button class="big-play" aria-label="播放" @click="toggle">
        <BaseIcon name="play" :size="35" /></button
      ><button
        class="du-btn du-btn-ghost du-btn-circle skip-button"
        aria-label="快进 10 秒"
        @click="seek(time + 10)"
      >
        <BaseIcon name="forward" :size="25" /><small>10</small>
      </button>
    </div>
    <span
      v-if="buffering && playing && phase === 'ready'"
      class="du-loading du-loading-spinner buffer-indicator"
      aria-label="正在缓冲"
    />
    <aside v-if="panel" class="player-panel">
      <header>
        <h2>
          {{ panel === "episodes" ? "选集" : panel === "settings" ? "播放设置" : "媒体信息" }}
        </h2>
        <button
          class="du-btn du-btn-ghost du-btn-circle du-btn-sm"
          aria-label="关闭面板"
          @click="panel = null"
        >
          <BaseIcon name="close" />
        </button>
      </header>
      <template v-if="panel === 'episodes'"
        ><div class="episode-queue">
          <button
            v-for="episode in playlist"
            :key="episode.id"
            :class="{ active: episode.id === item.id }"
            @click="emit('selectEpisode', episode.id)"
          >
            <div class="queue-image">
              <img
                v-if="episode.backdrop || episode.poster"
                :src="episode.backdrop || episode.poster"
                alt=""
              /><BaseIcon v-else name="tv" :size="25" /><span>{{
                String(episode.episodeNumber).padStart(2, "0")
              }}</span>
            </div>
            <div>
              <strong>第 {{ episode.seasonNumber }} 季 · 第 {{ episode.episodeNumber }} 集</strong
              ><small>{{
                episode.id === item.id
                  ? "正在观看"
                  : episode.duration
                    ? `${Math.round(episode.duration / 60)} 分钟`
                    : "播放这一集"
              }}</small>
            </div>
            <BaseIcon v-if="episode.id === item.id" name="play" :size="15" />
          </button></div></template
      ><template v-else-if="panel === 'settings'"
        ><label
          >播放速度<BaseSelect
            v-model="speed"
            :options="
              [0.5, 0.75, 1, 1.25, 1.5, 2].map((s) => ({ label: `${s}×`, value: String(s) }))
            " /></label
        ><label v-if="audioOptions.length"
          >音轨<BaseSelect
            v-model="audio"
            :options="audioOptions"
            @update:model-value="changeAudio" /></label
        ><label
          >字幕<BaseSelect
            v-model="subtitle"
            :options="subtitleOptions"
            @update:model-value="changeSubtitle"
        /></label>
        <p class="setting-note">
          {{
            subtitleOptions.length > 1
              ? "支持的内嵌字幕可在此切换。"
              : "当前媒体没有可切换的内嵌字幕。"
          }}
        </p>
        <button class="du-btn du-btn-ghost media-info-button" @click="panel = 'info'">
          <BaseIcon name="info" :size="17" />媒体信息<BaseIcon name="right" :size="16" /></button
      ></template>
      <dl v-else class="technical-info">
        <div>
          <dt>播放器</dt>
          <dd>webplayer</dd>
        </div>
        <div>
          <dt>分辨率</dt>
          <dd>{{ width && height ? `${width} × ${height}` : "—" }}</dd>
        </div>
        <div>
          <dt>时长</dt>
          <dd>{{ stamp(duration) }}</dd>
        </div>
        <div>
          <dt>播放方式</dt>
          <dd>{{ info?.native ? "浏览器原生播放" : "浏览器内重封装" }}</dd>
        </div>
        <div>
          <dt>音轨</dt>
          <dd>{{ audioOptions[Number(audio)]?.label || "—" }}</dd>
        </div>
      </dl>
    </aside>
    <footer class="player-bottom" :class="{ hidden: !visible && !panel && phase === 'ready' }">
      <p v-if="notice" class="player-notice" role="status">
        <BaseIcon name="info" :size="15" />{{ notice }}
      </p>
      <div class="bottom-caption">
        <div>
          <span>{{ item.subtitle || "私人媒体库" }}</span>
          <h2>{{ item.title }}</h2>
        </div>
        <button v-if="hasNextEpisode" class="du-btn next-episode" @click="emit('nextEpisode')">
          下一集<BaseIcon name="next" :size="17" />
        </button>
      </div>
      <div class="seek-row">
        <span>{{ stamp(scrubbing ? scrub : time) }}</span
        ><input
          class="du-range du-range-xs"
          type="range"
          min="0"
          :max="duration || 1"
          step=".1"
          :value="scrubbing ? scrub : time"
          :disabled="phase !== 'ready'"
          aria-label="播放进度"
          :style="{ '--range-progress': `${progress}%` }"
          @input="
            scrubbing = true;
            scrub = Number(($event.target as HTMLInputElement).value);
          "
          @change="seek(Number(($event.target as HTMLInputElement).value))"
        /><span>{{ stamp(duration) }}</span>
      </div>
      <div class="control-row">
        <div class="left-controls">
          <button
            class="du-btn du-btn-ghost du-btn-circle"
            :aria-label="playing ? '暂停' : '播放'"
            :disabled="phase !== 'ready'"
            @click="toggle"
          >
            <BaseIcon :name="playing ? 'pause' : 'play'" :size="23" /></button
          ><button
            v-if="playlist.length"
            class="du-btn du-btn-ghost du-btn-circle"
            aria-label="上一集"
            :disabled="!hasPreviousEpisode"
            @click="emit('previousEpisode')"
          >
            <BaseIcon name="previous" /></button
          ><button
            v-if="playlist.length"
            class="du-btn du-btn-ghost du-btn-circle"
            aria-label="下一集"
            :disabled="!hasNextEpisode"
            @click="emit('nextEpisode')"
          >
            <BaseIcon name="next" />
          </button>
          <div class="volume-control">
            <button
              class="du-btn du-btn-ghost du-btn-circle"
              :aria-label="muted ? '取消静音' : '静音'"
              @click="muted = !muted"
            >
              <BaseIcon :name="muted || !volume ? 'mute' : 'volume'" /></button
            ><input
              v-model.number="volume"
              class="du-range du-range-xs"
              type="range"
              min="0"
              max="1"
              step=".02"
              aria-label="音量"
            />
          </div>
        </div>
        <div class="right-controls">
          <button
            class="du-btn du-btn-ghost du-btn-circle"
            aria-label="音轨和字幕"
            @click="panel = panel === 'settings' ? null : 'settings'"
          >
            <BaseIcon name="subtitles" :size="22" /></button
          ><button
            class="du-btn du-btn-ghost du-btn-circle"
            aria-label="播放设置"
            @click="panel = panel === 'settings' ? null : 'settings'"
          >
            <BaseIcon name="settings" :size="21" /></button
          ><button
            class="du-btn du-btn-ghost du-btn-circle"
            :aria-label="fullscreen ? '退出全屏' : '进入全屏'"
            @click="toggleFullscreen"
          >
            <BaseIcon :name="fullscreen ? 'minimize' : 'fullscreen'" :size="21" />
          </button>
        </div>
      </div>
    </footer>
  </section>
</template>
<style scoped lang="scss">
.cinema-player {
  --fbz-color-bg: #080a0d;
  --fbz-color-panel: #171b21;
  --fbz-color-panel-strong: #242a32;
  --fbz-color-panel-elevated: #303842;
  --fbz-color-text: #f4f5f7;
  --fbz-color-text-soft: #c7ccd3;
  --fbz-color-text-muted: #9098a4;
  --fbz-color-line: #323a45;
  --fbz-color-line-soft: #242b35;
  position: fixed;
  inset: 0;
  z-index: 90;
  background: #030405;
  color: #f4f5f7;
  isolation: isolate;
  overflow: hidden;
}
.cinema-player video {
  width: 100%;
  height: 100%;
  object-fit: contain;
  display: block;
}
.player-veil {
  position: absolute;
  inset: 0;
  pointer-events: none;
  background: linear-gradient(180deg, #05080dc9, transparent 28%, transparent 46%, #05080dea);
  transition: opacity 0.25s;
}
.player-top {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  display: flex;
  align-items: center;
  gap: 18px;
  padding: 28px 36px;
  transition: opacity 0.25s;
}
.du-btn {
  color: #e8ebf0;
}
.du-btn:hover {
  background: #ffffff15;
}
.now-title {
  min-width: 0;
  flex: 1;
}
.now-title > span {
  color: #a8b0bc;
  font-size: 11px;
  letter-spacing: 0.5px;
}
.now-title h1 {
  font-size: 15px;
  font-weight: 550;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin: 5px 0 0;
  max-width: 70vw;
}
.quality-pill {
  border: 1px solid #ffffff35;
  padding: 3px 8px;
  font-size: 10px;
  border-radius: 5px;
  color: #cdd3dc;
}
.hidden {
  opacity: 0;
  pointer-events: none;
}
.player-state {
  position: absolute;
  left: 50%;
  top: 45%;
  transform: translate(-50%, -50%);
  text-align: center;
  display: grid;
  justify-items: center;
  gap: 18px;
  width: min(430px, calc(100% - 40px));
  padding: 30px;
  background: #090d13bb;
  backdrop-filter: blur(12px);
  border-radius: 18px;
}
.player-state h2 {
  font-size: 21px;
  font-weight: 550;
  margin: 0;
}
.player-state p {
  font-size: 13px;
  line-height: 1.8;
  color: #aeb6c3;
  margin: 0;
}
.player-state > div {
  display: flex;
  gap: 12px;
  margin-top: 8px;
}
.retry-button {
  background: #f3f4f5;
  color: #131820;
  border: 0;
}
.center-controls {
  position: absolute;
  top: 45%;
  left: 50%;
  transform: translate(-50%, -50%);
  display: flex;
  align-items: center;
  gap: 35px;
}
.big-play {
  width: 90px;
  height: 90px;
  border: 1.5px solid #ffffffe0;
  border-radius: 50%;
  display: grid;
  place-items: center;
  color: #fff;
  background: #0b101933;
  backdrop-filter: blur(4px);
  padding-left: 4px;
  transition: background 0.2s;
}
.big-play:hover {
  background: #ffffff25;
}
.skip-button {
  position: relative;
}
.skip-button small {
  position: absolute;
  font-size: 9px;
  bottom: 4px;
}
.buffer-indicator {
  position: absolute;
  top: 48%;
  left: 49%;
  color: white;
}
.player-bottom {
  position: absolute;
  inset: auto 0 0;
  padding: 20px 44px 25px;
  transition: opacity 0.25s;
}
.bottom-caption {
  display: flex;
  align-items: end;
  justify-content: space-between;
  margin-bottom: 23px;
  gap: 20px;
}
.bottom-caption span {
  font-size: 11px;
  color: #a8b0bc;
}
.bottom-caption h2 {
  font-size: 27px;
  letter-spacing: -0.6px;
  font-weight: 550;
  margin: 8px 0 0;
  max-width: 65vw;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.next-episode {
  font-size: 12px;
  background: #ffffff12;
  border: 1px solid #ffffff20;
}
.seek-row {
  display: flex;
  align-items: center;
  gap: 16px;
}
.seek-row > span {
  font-size: 11px;
  color: #c1c8d3;
  font-variant-numeric: tabular-nums;
  min-width: 45px;
}
.seek-row > span:last-child {
  text-align: right;
}
.du-range {
  --range-thumb: var(--fbz-color-brand-500);
  --range-bg: #ffffff28;
  --range-fill: 1;
  color: var(--fbz-color-brand-500);
  height: 15px;
}
.control-row,
.left-controls,
.right-controls,
.volume-control {
  display: flex;
  align-items: center;
  gap: 9px;
}
.control-row {
  justify-content: space-between;
  margin-top: 10px;
}
.control-row .du-btn {
  width: 38px;
  height: 38px;
  min-height: 0;
}
.volume-control .du-range {
  width: 85px;
  color: #dce1e8;
}
.player-notice {
  display: flex;
  align-items: center;
  gap: 8px;
  color: #e6c98b;
  font-size: 12px;
  margin-bottom: 16px;
}
.player-panel {
  position: absolute;
  right: 24px;
  top: 98px;
  bottom: 185px;
  width: 350px;
  background: #141a22ee;
  border: 1px solid #ffffff15;
  border-radius: 16px;
  backdrop-filter: blur(20px);
  padding: 22px;
  z-index: 3;
  overflow: auto;
}
.player-panel > header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 23px;
}
.player-panel h2 {
  font-size: 18px;
  font-weight: 550;
  margin: 0;
}
.player-panel label {
  display: grid;
  gap: 10px;
  font-size: 12px;
  color: #aeb7c3;
  margin-bottom: 22px;
}
.setting-note {
  font-size: 11px;
  line-height: 1.8;
  color: #8e98a7;
}
.media-info-button {
  width: 100%;
  justify-content: flex-start;
  margin-top: 25px;
  border-top: 1px solid #ffffff10;
  border-radius: 0;
  padding-top: 20px;
}
.media-info-button svg:last-child {
  margin-left: auto;
}
.episode-queue {
  display: grid;
  gap: 10px;
}
.episode-queue > button {
  display: flex;
  align-items: center;
  gap: 12px;
  text-align: left;
  padding: 8px;
  border: 1px solid transparent;
  border-radius: 10px;
  background: transparent;
  color: #c4ccd8;
}
.episode-queue > button.active {
  border-color: var(--fbz-color-brand-500);
  background: #ffffff06;
}
.queue-image {
  width: 82px;
  height: 53px;
  border-radius: 5px;
  overflow: hidden;
  position: relative;
  background: #252c37;
  display: grid;
  place-items: center;
  flex-shrink: 0;
}
.queue-image img {
  height: 100%;
  width: 100%;
  object-fit: cover;
}
.queue-image span {
  position: absolute;
  bottom: 3px;
  left: 5px;
  font-size: 10px;
  color: #fff;
  text-shadow: 0 1px 5px #000;
}
.episode-queue strong {
  display: block;
  font-size: 12px;
  font-weight: 550;
}
.episode-queue small {
  display: block;
  margin-top: 6px;
  font-size: 10px;
  color: #8994a4;
}
.technical-info > div {
  padding: 16px 0;
  border-bottom: 1px solid #ffffff10;
  display: flex;
  gap: 15px;
  justify-content: space-between;
  font-size: 12px;
}
.technical-info dt {
  color: #8f9aaa;
  white-space: nowrap;
}
.technical-info dd {
  text-align: right;
  margin: 0;
  color: #d5dbe5;
}
.cinema-player :is(button, input, select):focus-visible {
  outline: 2px solid var(--fbz-color-brand-500);
  outline-offset: 3px;
}
@media (max-width: 650px) {
  .player-top {
    padding: 20px 16px;
    gap: 9px;
  }
  .now-title h1 {
    font-size: 12px;
    max-width: 58vw;
  }
  .quality-pill {
    display: none;
  }
  .player-top > .du-btn {
    height: 34px;
    width: 34px;
    min-height: 0;
  }
  .player-bottom {
    padding: 18px 17px calc(15px + env(safe-area-inset-bottom));
  }
  .bottom-caption h2 {
    font-size: 18px;
    max-width: 85vw;
  }
  .bottom-caption {
    margin-bottom: 17px;
  }
  .next-episode {
    display: none;
  }
  .volume-control {
    display: none;
  }
  .seek-row {
    gap: 8px;
  }
  .seek-row > span {
    font-size: 10px;
    min-width: 33px;
  }
  .control-row {
    margin-top: 9px;
  }
  .center-controls {
    gap: 20px;
  }
  .big-play {
    width: 75px;
    height: 75px;
  }
  .player-panel {
    left: 0;
    right: 0;
    top: auto;
    bottom: 0;
    width: auto;
    max-height: 68dvh;
    border-radius: 18px 18px 0 0;
    padding: 22px 22px calc(25px + env(safe-area-inset-bottom));
  }
  .episode-queue {
    max-height: 48dvh;
    overflow: auto;
  }
  .player-state {
    top: 43%;
    padding: 24px;
  }
  .player-state h2 {
    font-size: 18px;
  }
  .player-state p {
    font-size: 12px;
  }
}
.seek-row .du-range {
  flex: 1;
  width: 100%;
  min-width: 0;
}
.player-top.hidden:focus-within,
.player-bottom.hidden:focus-within {
  opacity: 1;
  pointer-events: auto;
}
</style>
