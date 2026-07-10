<script setup lang="ts">
import {
  newPlaySessionId,
  reportPlaybackProgress,
  reportPlaybackStart,
  reportPlaybackStopped,
} from "@/service/modules/playbackReport.ts";
import type {
  PlaybackChapter,
  PlaybackEpisode,
  PlaybackItem,
  PlaybackTrack,
} from "@/stores/playback.ts";

type ShakaApi = typeof import("shaka-player/dist/shaka-player.compiled").default;
type ShakaPlayerInstance = InstanceType<ShakaApi["Player"]>;

interface MediaStat {
  label: string;
  value: string;
}

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

const videoRef = shallowRef<HTMLVideoElement>();
const shakaPlayer = shallowRef<ShakaPlayerInstance>();
const controlsVisible = shallowRef(true);
const infoPanelOpen = shallowRef(true);
const settingsOpen = shallowRef(false);
const isPlaying = shallowRef(false);
const isBuffering = shallowRef(false);
const currentTime = shallowRef(0);
const videoDuration = shallowRef(0);
const volume = shallowRef(0.82);
const selectedAudioTrack = shallowRef("");
const selectedSubtitleTrack = shallowRef("off");
const loadState = shallowRef("等待媒体源");
const loadError = shallowRef("");
const selectedInfoTab = shallowRef<"episodes" | "chapters" | "info">("episodes");
// 真实音轨/字幕（媒体加载后从 shaka 读出；无流时为空并显示空态）。
const realAudioTracks = ref<PlaybackTrack[]>([]);
const realSubtitleTracks = ref<PlaybackTrack[]>([]);

let hideTimer: number | undefined;
let shakaApiPromise: Promise<ShakaApi> | undefined;
// 播放进度上报：一次播放（open→close/切集）一个会话 id，周期 + 关键事件上报。
let progressTimer: number | undefined;
let playSessionId = "";
let reportedItemId = "";

const hasSource = computed(() => Boolean(props.item.source?.uri));
const duration = computed(() => props.item.duration ?? (videoDuration.value || 45 * 60));
const progressPercent = computed(() => {
  if (!duration.value) return 0;
  return Math.min(100, Math.max(0, (currentTime.value / duration.value) * 100));
});

const audioTracks = computed<PlaybackTrack[]>(() =>
  props.item.audioTracks?.length ? props.item.audioTracks : realAudioTracks.value,
);

const subtitleTracks = computed<PlaybackTrack[]>(() => {
  if (props.item.subtitleTracks?.length) return props.item.subtitleTracks;
  if (!realSubtitleTracks.value.length) return [];
  return [{ id: "off", label: "关闭字幕" }, ...realSubtitleTracks.value];
});

const chapters = computed<PlaybackChapter[]>(() => {
  if (props.item.chapters?.length) return props.item.chapters;
  return [];
});

const mediaStats = computed<MediaStat[]>(() => {
  const player = shakaPlayer.value;
  const stats = player?.getStats();
  const variantTracks = player?.getVariantTracks() ?? [];
  const activeTrack = variantTracks.find((track) => track.active);
  return [
    { label: "播放器核心", value: "Shaka Player 5.1" },
    {
      label: "协议",
      value: props.item.source?.mimeType ?? (hasSource.value ? "自适应流" : "无媒体源"),
    },
    {
      label: "分辨率",
      value: activeTrack?.height ? `${activeTrack.width}x${activeTrack.height}` : "待播放器上报",
    },
    {
      label: "带宽估算",
      value: stats?.estimatedBandwidth
        ? `${Math.round(stats.estimatedBandwidth / 1000)} Kbps`
        : "待播放器上报",
    },
    {
      label: "缓冲",
      value: stats?.bufferingTime != null ? `${Math.round(stats.bufferingTime)}s` : "待播放器上报",
    },
    {
      label: "音轨",
      value:
        audioTracks.value.find((track) => track.id === selectedAudioTrack.value)?.label ?? "默认",
    },
    {
      label: "字幕",
      value:
        subtitleTracks.value.find((track) => track.id === selectedSubtitleTrack.value)?.label ??
        "关闭",
    },
  ];
});

watch(
  () => [props.item.id, props.item.source?.uri] as const,
  async () => {
    await resetPlayer();
  },
  { immediate: true },
);

watch(volume, (value) => {
  if (videoRef.value) videoRef.value.volume = value;
});

onMounted(() => {
  useEventListener(window, "keydown", handleKeydown);
});

onBeforeUnmount(() => {
  clearTimers();
  void reportStopped();
  void shakaPlayer.value?.destroy();
});

async function resetPlayer() {
  clearTimers();
  // 切换条目前把上一条的最终进度落掉。
  await reportStopped();
  currentTime.value = 0;
  videoDuration.value = 0;
  isPlaying.value = false;
  isBuffering.value = false;
  loadError.value = "";
  realAudioTracks.value = [];
  realSubtitleTracks.value = [];
  selectedAudioTrack.value = "";
  selectedSubtitleTrack.value = "off";
  loadState.value = "初始化播放核心";

  await nextTick();
  const video = videoRef.value;
  if (!video) return;

  video.volume = volume.value;
  video.poster = props.item.backdrop ?? props.item.poster ?? "";

  if (shakaPlayer.value) {
    await shakaPlayer.value.destroy();
    shakaPlayer.value = undefined;
  }

  if (!hasSource.value) {
    loadState.value = "当前资源未提供可播放流地址";
    return;
  }

  try {
    const shakaApi = await loadShaka();
    shakaApi.polyfill.installAll();
    const player = new shakaApi.Player(video);
    shakaPlayer.value = player;
    player.configure({
      streaming: {
        bufferingGoal: 18,
        rebufferingGoal: 2,
      },
    });
    player.addEventListener("buffering", (event) => {
      isBuffering.value = Boolean((event as CustomEvent<boolean>).detail);
    });
    // 续播：直接从上次位置开始加载（比 load 后 seek 少一次缓冲往返）。
    const startAt = props.item.startPositionSeconds;
    await player.load(props.item.source!.uri!, startAt && startAt > 0 ? startAt : undefined);
    videoDuration.value = video.duration || props.item.duration || 0;
    refreshRealTracks();
    loadState.value = "媒体已就绪";

    // 开始上报播放会话（进度写入 user_playstates → 继续观看）。
    playSessionId = newPlaySessionId();
    reportedItemId = props.item.id;
    void reportPlaybackStart({
      itemId: reportedItemId,
      playSessionId,
      positionSeconds: startAt ?? 0,
    }).catch(() => {});
    startProgressTimer();
  } catch (error) {
    loadError.value = error instanceof Error ? error.message : "媒体加载失败";
    loadState.value = "媒体加载失败";
  }
}

/** 从 shaka 读取真实音轨（按语言聚合）与字幕轨。 */
function refreshRealTracks() {
  const player = shakaPlayer.value;
  if (!player) return;

  const languages = player.getAudioLanguages();
  const activeVariant = player.getVariantTracks().find((track) => track.active);
  realAudioTracks.value = languages.map((language) => ({
    id: language,
    label: languageLabel(language),
    language,
    active: activeVariant?.language === language,
  }));
  if (activeVariant?.language) selectedAudioTrack.value = activeVariant.language;

  const textTracks = player.getTextTracks();
  realSubtitleTracks.value = textTracks.map((track, index) => ({
    id: String(track.id),
    label: track.label || languageLabel(track.language ?? "") || `字幕 ${index + 1}`,
    language: track.language ?? undefined,
    active: track.active,
  }));
  const activeText = textTracks.find((track) => track.active);
  selectedSubtitleTrack.value =
    activeText && player.isTextTrackVisible() ? String(activeText.id) : "off";
}

/** 语言码 → 展示名（浏览器 Intl 有则用，缺省回显原码）。 */
function languageLabel(language: string): string {
  if (!language) return "";
  try {
    const display = new Intl.DisplayNames(["zh-CN"], { type: "language" });
    return display.of(language) ?? language;
  } catch {
    return language;
  }
}

/** 切换音轨（按语言）。 */
function selectAudioTrack(track: PlaybackTrack) {
  selectedAudioTrack.value = track.id;
  if (track.language) shakaPlayer.value?.selectAudioLanguage(track.language);
}

/** 切换字幕轨（off = 关闭显示）。 */
function selectSubtitleTrack(track: PlaybackTrack) {
  selectedSubtitleTrack.value = track.id;
  const player = shakaPlayer.value;
  if (!player) return;
  if (track.id === "off") {
    void player.setTextTrackVisibility(false);
    return;
  }
  const target = player.getTextTracks().find((candidate) => String(candidate.id) === track.id);
  if (target) {
    player.selectTextTrack(target);
    void player.setTextTrackVisibility(true);
  }
}

function startProgressTimer() {
  stopProgressTimer();
  progressTimer = window.setInterval(() => {
    void reportProgress();
  }, 10_000);
}

function stopProgressTimer() {
  if (progressTimer) window.clearInterval(progressTimer);
  progressTimer = undefined;
}

async function reportProgress() {
  if (!playSessionId || !reportedItemId) return;
  try {
    await reportPlaybackProgress({
      itemId: reportedItemId,
      playSessionId,
      positionSeconds: currentTime.value,
      isPaused: !isPlaying.value,
    });
  } catch {
    // 进度上报失败不打断播放。
  }
}

async function reportStopped() {
  stopProgressTimer();
  if (!playSessionId || !reportedItemId) return;
  const sessionId = playSessionId;
  const itemId = reportedItemId;
  playSessionId = "";
  reportedItemId = "";
  try {
    await reportPlaybackStopped({
      itemId,
      playSessionId: sessionId,
      positionSeconds: currentTime.value,
    });
  } catch {
    // 停止上报失败不打断关闭流程。
  }
}

async function loadShaka() {
  shakaApiPromise ??= import("shaka-player/dist/shaka-player.compiled").then((mod) => mod.default);
  return shakaApiPromise;
}

function clearTimers() {
  if (hideTimer) window.clearTimeout(hideTimer);
  hideTimer = undefined;
  stopProgressTimer();
}

function handlePointerMove() {
  controlsVisible.value = true;
  if (hideTimer) window.clearTimeout(hideTimer);
  hideTimer = window.setTimeout(() => {
    if (isPlaying.value && !infoPanelOpen.value && !settingsOpen.value)
      controlsVisible.value = false;
  }, 2600);
}

async function togglePlay() {
  if (hasSource.value && videoRef.value && !loadError.value) {
    if (videoRef.value.paused) {
      await videoRef.value.play();
    } else {
      videoRef.value.pause();
    }
    return;
  }

  loadError.value = "当前资源没有来自后端的播放地址。";
}

function onVideoPlay() {
  isPlaying.value = true;
}

function onVideoPause() {
  isPlaying.value = false;
  // 暂停即落一次进度（用户此时最可能离开）。
  void reportProgress();
}

function onVideoEnded() {
  isPlaying.value = false;
  currentTime.value = duration.value;
  void reportStopped();
  // 剧集看完自动接下一集。
  if (props.hasNextEpisode) emit("nextEpisode");
}

function onTimeUpdate() {
  if (!videoRef.value) return;
  currentTime.value = videoRef.value.currentTime;
}

function onLoadedMetadata() {
  if (!videoRef.value) return;
  videoDuration.value = videoRef.value.duration || 0;
}

function seekTo(value: number | string) {
  const next = Number(value);
  currentTime.value = next;
  if (videoRef.value && hasSource.value) videoRef.value.currentTime = next;
}

function seekBy(delta: number) {
  seekTo(Math.min(duration.value, Math.max(0, currentTime.value + delta)));
}

function jumpToChapter(chapter: PlaybackChapter) {
  seekTo(chapter.startTime);
}

function formatTime(seconds: number) {
  const safeSeconds = Math.max(0, Math.floor(seconds));
  const minutes = Math.floor(safeSeconds / 60);
  const remainingSeconds = safeSeconds % 60;
  return `${minutes}:${String(remainingSeconds).padStart(2, "0")}`;
}

function handleKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
  if (event.key === " ") {
    event.preventDefault();
    void togglePlay();
  }
  if (event.key === "ArrowLeft") seekBy(-10);
  if (event.key === "ArrowRight") seekBy(10);
}
</script>

<template>
  <section
    class="fullscreen-player"
    :class="{ idle: !controlsVisible }"
    aria-modal="true"
    role="dialog"
    @pointermove="handlePointerMove"
  >
    <video
      ref="videoRef"
      class="video-element"
      playsinline
      :poster="props.item.backdrop ?? props.item.poster"
      @play="onVideoPlay"
      @pause="onVideoPause"
      @ended="onVideoEnded"
      @timeupdate="onTimeUpdate"
      @loadedmetadata="onLoadedMetadata"
      @waiting="isBuffering = true"
      @playing="isBuffering = false"
      @click="togglePlay"
    />

    <img
      v-if="!hasSource && (props.item.backdrop || props.item.poster)"
      class="fallback-art"
      :src="props.item.backdrop ?? props.item.poster"
      :alt="props.item.title"
    />

    <div class="screen-gradient" />

    <button class="close-button" type="button" aria-label="关闭播放" @click="emit('close')">
      ×
    </button>

    <div class="top-info">
      <p>{{ loadState }}</p>
      <span v-if="loadError">{{ loadError }}</span>
    </div>

    <button
      class="center-play"
      type="button"
      :aria-label="isPlaying ? '暂停' : '播放'"
      :disabled="!hasSource || Boolean(loadError)"
      @click="togglePlay"
    >
      {{ isPlaying ? "Ⅱ" : "▶" }}
    </button>

    <div v-if="isBuffering" class="buffering">
      <span />
      <p>正在缓冲媒体数据</p>
    </div>

    <div class="control-layer">
      <div class="title-block">
        <p>正在播放</p>
        <h2>{{ props.item.title }}</h2>
        <span v-if="props.item.subtitle">{{ props.item.subtitle }}</span>
      </div>

      <div class="timeline">
        <span>{{ formatTime(currentTime) }}</span>
        <input
          class="seek"
          type="range"
          min="0"
          :max="duration"
          step="1"
          :value="currentTime"
          :style="{ '--progress': `${progressPercent}%` }"
          aria-label="播放进度"
          @input="seekTo(($event.target as HTMLInputElement).value)"
        />
        <span>{{ formatTime(duration) }}</span>
      </div>

      <div class="control-row">
        <div class="left-controls">
          <button
            type="button"
            @click="emit('previousEpisode')"
            :disabled="!props.hasPreviousEpisode"
          >
            上一集
          </button>
          <button
            class="primary-control"
            type="button"
            :disabled="!hasSource || Boolean(loadError)"
            @click="togglePlay"
          >
            {{ isPlaying ? "暂停" : "播放" }}
          </button>
          <button type="button" @click="emit('nextEpisode')" :disabled="!props.hasNextEpisode">
            下一集
          </button>
          <button type="button" @click="seekBy(-10)">-10s</button>
          <button type="button" @click="seekBy(10)">+10s</button>
        </div>

        <div class="right-controls">
          <label class="volume-control">
            <span>音量</span>
            <input
              v-model.number="volume"
              class="volume-slider"
              type="range"
              min="0"
              max="1"
              step="0.01"
              :style="{ '--progress': `${volume * 100}%` }"
              aria-label="音量"
            />
          </label>
          <button type="button" @click="settingsOpen = !settingsOpen">音轨/字幕</button>
          <button type="button" @click="infoPanelOpen = !infoPanelOpen">
            {{ infoPanelOpen ? "隐藏信息" : "选集信息" }}
          </button>
        </div>
      </div>
    </div>

    <div v-if="settingsOpen" class="settings-popover">
      <section>
        <h3>音轨</h3>
        <button
          v-for="track in audioTracks"
          :key="track.id"
          type="button"
          :class="{ active: selectedAudioTrack === track.id }"
          @click="selectAudioTrack(track)"
        >
          {{ track.label }}
        </button>
        <p v-if="!audioTracks.length" class="empty-note">当前媒体未上报可切换音轨。</p>
      </section>
      <section>
        <h3>字幕</h3>
        <button
          v-for="track in subtitleTracks"
          :key="track.id"
          type="button"
          :class="{ active: selectedSubtitleTrack === track.id }"
          @click="selectSubtitleTrack(track)"
        >
          {{ track.label }}
        </button>
        <p v-if="!subtitleTracks.length" class="empty-note">当前媒体没有内嵌/外挂字幕轨。</p>
      </section>
    </div>

    <aside v-if="infoPanelOpen" class="info-dock">
      <div class="dock-tabs">
        <button
          type="button"
          :class="{ active: selectedInfoTab === 'episodes' }"
          @click="selectedInfoTab = 'episodes'"
        >
          选集
        </button>
        <button
          type="button"
          :class="{ active: selectedInfoTab === 'chapters' }"
          @click="selectedInfoTab = 'chapters'"
        >
          章节
        </button>
        <button
          type="button"
          :class="{ active: selectedInfoTab === 'info' }"
          @click="selectedInfoTab = 'info'"
        >
          加载信息
        </button>
      </div>

      <div v-if="selectedInfoTab === 'episodes'" class="episode-rail">
        <button
          v-for="episode in props.playlist"
          :key="episode.id"
          type="button"
          :class="{ active: episode.id === props.item.id }"
          @click="emit('selectEpisode', episode.id)"
        >
          <strong>S{{ episode.seasonNumber }} E{{ episode.episodeNumber }}</strong>
          <span>{{ episode.title }}</span>
        </button>
        <p v-if="!props.playlist.length" class="empty-note">当前媒体没有可切换分集。</p>
      </div>

      <div v-else-if="selectedInfoTab === 'chapters'" class="chapter-list">
        <button
          v-for="chapter in chapters"
          :key="chapter.id"
          type="button"
          :class="{
            active:
              currentTime >= chapter.startTime &&
              currentTime < chapter.startTime + chapter.duration,
          }"
          @click="jumpToChapter(chapter)"
        >
          <strong>{{ chapter.title }}</strong>
          <span
            >{{ formatTime(chapter.startTime) }} -
            {{ formatTime(chapter.startTime + chapter.duration) }}</span
          >
        </button>
        <p v-if="!chapters.length" class="empty-note">当前媒体没有后端章节数据。</p>
      </div>

      <dl v-else class="media-stats">
        <div v-for="stat in mediaStats" :key="stat.label">
          <dt>{{ stat.label }}</dt>
          <dd>{{ stat.value }}</dd>
        </div>
      </dl>
    </aside>
  </section>
</template>

<style scoped lang="scss">
.fullscreen-player {
  position: fixed;
  inset: 0;
  z-index: calc(var(--fbz-z-overlay) + 80);
  overflow: hidden;
  background: #030304;
  color: #fff;
  cursor: default;
}

.video-element,
.fallback-art {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
  background: #030304;
}

.video-element {
  z-index: 1;
}

.fallback-art {
  z-index: 2;
  opacity: 0.74;
  filter: saturate(0.92);
}

.screen-gradient {
  position: absolute;
  inset: 0;
  z-index: 3;
  pointer-events: none;
  background:
    linear-gradient(
      180deg,
      rgba(0, 0, 0, 0.64) 0%,
      rgba(0, 0, 0, 0.08) 36%,
      rgba(0, 0, 0, 0.78) 100%
    ),
    radial-gradient(circle at 50% 48%, rgba(0, 0, 0, 0), rgba(0, 0, 0, 0.45) 68%);
}

.close-button,
.top-info,
.center-play,
.buffering,
.control-layer,
.settings-popover,
.info-dock {
  position: absolute;
  z-index: 4;
}

.close-button {
  top: 22px;
  right: 24px;
  width: 42px;
  height: 42px;
  border: 1px solid rgba(255, 255, 255, 0.16);
  border-radius: 50%;
  background: rgba(0, 0, 0, 0.52);
  color: #fff;
  font-size: 26px;
  line-height: 1;
  cursor: pointer;
  backdrop-filter: blur(14px);
  -webkit-backdrop-filter: blur(14px);
}

.top-info {
  top: 24px;
  left: 28px;
  display: flex;
  align-items: center;
  gap: var(--fbz-space-3);
  color: rgba(255, 255, 255, 0.76);
  font-size: var(--fbz-font-size-sm);

  p,
  span {
    margin: 0;
  }

  span {
    color: #ffb4b4;
  }
}

.center-play {
  left: 50%;
  top: 50%;
  width: 82px;
  height: 82px;
  display: grid;
  place-content: center;
  padding-left: 4px;
  border: 1px solid rgba(255, 255, 255, 0.2);
  border-radius: 50%;
  background: color-mix(in srgb, var(--fbz-color-brand-500) 92%, transparent);
  color: #07120a;
  font-size: 28px;
  transform: translate(-50%, -50%);
  cursor: pointer;
  box-shadow: 0 22px 62px rgba(0, 0, 0, 0.38);
  transition:
    transform var(--fbz-motion-fast) ease,
    background var(--fbz-motion-fast) ease;

  &:hover {
    background: var(--fbz-color-brand-500);
    transform: translate(-50%, -50%) scale(1.08);
  }
}

.buffering {
  left: 50%;
  top: calc(50% + 74px);
  display: flex;
  align-items: center;
  gap: var(--fbz-space-2);
  transform: translateX(-50%);
  color: rgba(255, 255, 255, 0.78);

  span {
    width: 18px;
    height: 18px;
    border: 2px solid rgba(255, 255, 255, 0.24);
    border-top-color: var(--fbz-color-brand-500);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  p {
    margin: 0;
    font-size: var(--fbz-font-size-sm);
  }
}

.control-layer {
  inset: auto 0 0;
  padding: 0 28px 24px;
  transition:
    opacity var(--fbz-motion-fast),
    transform var(--fbz-motion-fast);
}

.title-block {
  max-width: min(720px, calc(100vw - 56px));
  margin-bottom: var(--fbz-space-4);

  p,
  h2,
  span {
    margin: 0;
  }

  p {
    margin-bottom: var(--fbz-space-1);
    color: var(--fbz-color-brand-500);
    font-size: var(--fbz-font-size-sm);
    font-weight: 800;
  }

  h2 {
    font-size: clamp(28px, 4vw, 54px);
    line-height: 1.05;
    letter-spacing: 0;
  }

  span {
    display: block;
    margin-top: var(--fbz-space-2);
    color: rgba(255, 255, 255, 0.72);
  }
}

.timeline {
  display: grid;
  grid-template-columns: 48px minmax(0, 1fr) 48px;
  align-items: center;
  gap: var(--fbz-space-3);
  color: rgba(255, 255, 255, 0.78);
  font-size: var(--fbz-font-size-sm);
}

.seek,
.volume-slider {
  -webkit-appearance: none;
  appearance: none;
  background: transparent;
  cursor: pointer;
  height: 20px;
  display: flex;
  align-items: center;

  &:focus {
    outline: none;
  }

  // Webkit Track
  &::-webkit-slider-runnable-track {
    background: linear-gradient(
      to right,
      var(--fbz-color-brand-500) 0%,
      var(--fbz-color-brand-500) var(--progress),
      rgba(255, 255, 255, 0.2) var(--progress),
      rgba(255, 255, 255, 0.2) 100%
    );
    height: 4px;
    border-radius: 2px;
    transition: height var(--fbz-motion-fast) ease;
  }

  // Webkit Thumb
  &::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    background: #ffffff;
    height: 12px;
    width: 12px;
    border-radius: 50%;
    margin-top: -4px; // Center (4px height - 12px thumb = -8px / 2 = -4px)
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
    transition:
      transform var(--fbz-motion-fast) ease,
      background var(--fbz-motion-fast) ease;
  }

  // Firefox Track
  &::-moz-range-track {
    background: linear-gradient(
      to right,
      var(--fbz-color-brand-500) 0%,
      var(--fbz-color-brand-500) var(--progress),
      rgba(255, 255, 255, 0.2) var(--progress),
      rgba(255, 255, 255, 0.2) 100%
    );
    height: 4px;
    border-radius: 2px;
    transition: height var(--fbz-motion-fast) ease;
  }

  // Firefox Thumb
  &::-moz-range-thumb {
    background: #ffffff;
    height: 12px;
    width: 12px;
    border: none;
    border-radius: 50%;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
    transition:
      transform var(--fbz-motion-fast) ease,
      background var(--fbz-motion-fast) ease;
  }

  &:hover {
    &::-webkit-slider-runnable-track {
      height: 6px;
    }
    &::-webkit-slider-thumb {
      transform: scale(1.3);
      background: var(--fbz-color-brand-500);
      margin-top: -5px;
    }
    &::-moz-range-track {
      height: 6px;
    }
    &::-moz-range-thumb {
      transform: scale(1.3);
      background: var(--fbz-color-brand-500);
    }
  }
}

.volume-slider {
  width: 80px;
  transition: width var(--fbz-motion-base) ease;

  &:hover,
  &:focus-within {
    width: 110px;
  }
}

.control-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--fbz-space-4);
  margin-top: var(--fbz-space-4);
}

.left-controls,
.right-controls,
.volume-control {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-2);
}

.left-controls button,
.right-controls button,
.settings-popover button,
.dock-tabs button,
.episode-rail button,
.chapter-list button {
  border: 1px solid rgba(255, 255, 255, 0.13);
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.08);
  color: rgba(255, 255, 255, 0.88);
  cursor: pointer;
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  transition:
    background var(--fbz-motion-fast) ease,
    border-color var(--fbz-motion-fast) ease,
    color var(--fbz-motion-fast) ease;

  &:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.16);
    border-color: rgba(255, 255, 255, 0.25);
    color: #ffffff;
  }
}

.left-controls button,
.right-controls button {
  min-height: 36px;
  padding: 0 12px;
  font-weight: 700;

  &:disabled {
    cursor: not-allowed;
    opacity: 0.36;
  }
}

.primary-control {
  background: color-mix(in srgb, var(--fbz-color-brand-500) 92%, transparent) !important;
  color: #07120a !important;
  border-color: transparent !important;

  &:hover:not(:disabled) {
    background: var(--fbz-color-brand-500) !important;
    color: #000000 !important;
    box-shadow: 0 0 12px color-mix(in srgb, var(--fbz-color-brand-500) 40%, transparent);
  }
}

.volume-control {
  color: rgba(255, 255, 255, 0.72);
  font-size: var(--fbz-font-size-sm);
}

.settings-popover {
  right: 28px;
  bottom: 102px;
  width: min(360px, calc(100vw - 56px));
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--fbz-space-3);
  padding: var(--fbz-space-4);
  border: 1px solid rgba(255, 255, 255, 0.14);
  border-radius: 8px;
  background: rgba(9, 9, 11, 0.84);
  box-shadow: 0 24px 70px rgba(0, 0, 0, 0.42);
  backdrop-filter: blur(24px);
  -webkit-backdrop-filter: blur(24px);

  h3 {
    margin: 0 0 var(--fbz-space-2);
    font-size: var(--fbz-font-size-sm);
  }

  button {
    width: 100%;
    min-height: 34px;
    margin-top: var(--fbz-space-2);
    text-align: left;
    padding: 0 10px;
  }
}

.settings-popover .active,
.dock-tabs .active,
.episode-rail .active,
.chapter-list .active {
  border-color: color-mix(in srgb, var(--fbz-color-brand-500) 56%, transparent) !important;
  background: color-mix(in srgb, var(--fbz-color-brand-500) 16%, transparent) !important;
  color: #fff !important;
}

.info-dock {
  left: 28px;
  right: 28px;
  bottom: 118px;
  display: grid;
  grid-template-columns: 160px minmax(0, 1fr);
  gap: var(--fbz-space-4);
  max-height: 260px;
  padding: var(--fbz-space-4);
  border: 1px solid rgba(255, 255, 255, 0.14);
  border-radius: 8px;
  background: rgba(8, 8, 10, 0.72);
  box-shadow: 0 24px 70px rgba(0, 0, 0, 0.36);
  backdrop-filter: blur(24px);
  -webkit-backdrop-filter: blur(24px);
}

.dock-tabs {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);

  button {
    min-height: 42px;
    text-align: left;
    padding: 0 12px;
    font-weight: 800;
  }
}

.episode-rail {
  display: grid;
  grid-auto-flow: column;
  grid-auto-columns: minmax(132px, 172px);
  gap: var(--fbz-space-3);
  overflow-x: auto;
  padding-bottom: var(--fbz-space-2);

  button {
    min-height: 92px;
    padding: 12px;
    text-align: left;
  }

  strong,
  span {
    display: block;
  }

  span {
    margin-top: var(--fbz-space-2);
    color: rgba(255, 255, 255, 0.68);
    font-size: var(--fbz-font-size-sm);
  }
}

.chapter-list {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: var(--fbz-space-3);

  button {
    min-height: 72px;
    padding: 10px 12px;
    text-align: left;
  }

  strong,
  span {
    display: block;
  }

  span {
    margin-top: var(--fbz-space-1);
    color: rgba(255, 255, 255, 0.62);
    font-size: var(--fbz-font-size-sm);
  }
}

.media-stats {
  margin: 0;
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: var(--fbz-space-3);

  div {
    min-height: 70px;
    padding: 10px 12px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.06);
  }

  dt {
    color: rgba(255, 255, 255, 0.56);
    font-size: var(--fbz-font-size-xs);
  }

  dd {
    margin: 6px 0 0;
    color: #fff;
    font-size: var(--fbz-font-size-sm);
    font-weight: 800;
  }
}

.empty-note {
  margin: 0;
  color: rgba(255, 255, 255, 0.62);
}

.fullscreen-player.idle {
  cursor: none;

  .control-layer,
  .center-play,
  .top-info,
  .close-button {
    opacity: 0;
    pointer-events: none;
  }

  .control-layer {
    transform: translateY(14px);
  }
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

@media (max-width: 900px) {
  .control-row,
  .right-controls {
    align-items: stretch;
    flex-direction: column;
  }

  .control-row {
    align-items: stretch;
  }

  .info-dock {
    grid-template-columns: 1fr;
    max-height: 42vh;
    overflow-y: auto;
  }

  .dock-tabs {
    flex-direction: row;
  }

  .chapter-list,
  .media-stats {
    grid-template-columns: 1fr 1fr;
  }
}

@media (max-width: 560px) {
  .control-layer {
    padding: 0 16px 18px;
  }

  .left-controls {
    flex-wrap: wrap;
  }

  .info-dock {
    left: 16px;
    right: 16px;
    bottom: 168px;
  }

  .chapter-list,
  .media-stats {
    grid-template-columns: 1fr;
  }
}
</style>
