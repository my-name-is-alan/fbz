<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useMusicPlayerStore } from "@/stores/musicPlayer.ts";
import { formatDuration } from "@/service/modules/music.ts";
import {
  newPlaySessionId,
  reportPlaybackProgress,
  reportPlaybackStart,
  reportPlaybackStopped,
} from "@/service/modules/playbackReport.ts";

/**
 * 底部音乐播放栏 —— 全局挂载，仅在有当前曲目时显示。
 * 用原生 `<audio>` 播放后端直出流（RANGE 支持，可拖动进度）；与视频全屏播放器并存互不干扰。
 * 同时把播放进度上报后端（开始/进度/停止），让曲目进入「继续观看」。
 */
const player = useMusicPlayerStore();
const { current, streamUrl, hasPrevious, hasNext } = storeToRefs(player);

const audioRef = ref<HTMLAudioElement>();
const isPlaying = ref(false);
const currentTime = ref(0);
const duration = ref(0);

/** 当前播放会话：曲目 id + 客户端会话标识，贯穿一次播放的开始→进度→停止。 */
let reportedItemId: string | undefined;
let playSessionId = "";
/** 进度上报节流：最近一次上报的秒数（每 ~10s 上报一次，避免刷屏）。 */
let lastProgressReportedAt = 0;
const PROGRESS_REPORT_INTERVAL_SECONDS = 10;

/** 给上一首（reportedItemId）补一条 stopped（落最终位置），best-effort 不阻断 UI。 */
function reportStoppedForCurrent(positionSeconds: number) {
  if (!reportedItemId) return;
  const itemId = reportedItemId;
  const session = playSessionId;
  reportedItemId = undefined;
  void reportPlaybackStopped({ itemId, playSessionId: session, positionSeconds }).catch(() => {});
}

/** 切歌时（streamUrl 变化）：旧曲 stopped、新曲加载播放 + start。 */
watch(streamUrl, async (url) => {
  // 旧曲先收尾（用切换前的播放位置）。
  reportStoppedForCurrent(currentTime.value);

  if (!url) return;
  await nextTick();
  const el = audioRef.value;
  if (!el) return;
  el.src = url;
  currentTime.value = 0;
  duration.value = 0;

  // 新曲会话：记录 id + 新会话标识，上报 start（best-effort）。
  reportedItemId = current.value?.id;
  playSessionId = newPlaySessionId();
  lastProgressReportedAt = 0;
  if (reportedItemId) {
    void reportPlaybackStart({
      itemId: reportedItemId,
      playSessionId,
      positionSeconds: 0,
    }).catch(() => {});
  }

  try {
    await el.play();
  } catch {
    // 自动播放被浏览器拦截（无用户手势）：保持暂停态，用户可手动点播放。
    isPlaying.value = false;
  }
});

/** 周期上报播放进度（节流到 ~10s 一次）。 */
function onTimeUpdate() {
  const el = audioRef.value;
  if (!el) return;
  currentTime.value = el.currentTime;
  if (!reportedItemId) return;
  if (currentTime.value - lastProgressReportedAt < PROGRESS_REPORT_INTERVAL_SECONDS) return;
  lastProgressReportedAt = currentTime.value;
  void reportPlaybackProgress({
    itemId: reportedItemId,
    playSessionId,
    positionSeconds: currentTime.value,
    isPaused: el.paused,
  }).catch(() => {});
}

/** 暂停/恢复也上报一次进度（携带 isPaused），让后端状态及时。 */
function reportPauseState(isPaused: boolean) {
  if (!reportedItemId) return;
  void reportPlaybackProgress({
    itemId: reportedItemId,
    playSessionId,
    positionSeconds: currentTime.value,
    isPaused,
  }).catch(() => {});
}

function togglePlay() {
  const el = audioRef.value;
  if (!el) return;
  if (el.paused) void el.play();
  else el.pause();
}

/** 拖动进度条 seek。 */
function onSeek(event: Event) {
  const el = audioRef.value;
  const target = event.target as HTMLInputElement;
  if (!el) return;
  el.currentTime = Number(target.value);
}

/** 播完自动下一首；末尾停下并上报 stopped。 */
function onEnded() {
  if (hasNext.value) {
    player.playNext();
  } else {
    reportStoppedForCurrent(currentTime.value);
    isPlaying.value = false;
  }
}

/** 关闭播放栏：先收尾上报，再清空队列。 */
function closeBar() {
  reportStoppedForCurrent(currentTime.value);
  player.close();
}

// 组件卸载（如登出跳转）也收尾上报，避免悬挂的播放会话。
onBeforeUnmount(() => reportStoppedForCurrent(currentTime.value));

const progress = computed(() => (duration.value > 0 ? currentTime.value / duration.value : 0));
</script>

<template>
  <Teleport to="body">
    <Transition name="music-bar-slide">
      <div v-if="current" class="music-bar" role="region" aria-label="音乐播放栏">
        <audio
          ref="audioRef"
          @play="
            isPlaying = true;
            reportPauseState(false);
          "
          @pause="
            isPlaying = false;
            reportPauseState(true);
          "
          @timeupdate="onTimeUpdate"
          @loadedmetadata="duration = audioRef?.duration ?? 0"
          @ended="onEnded"
        />

        <div class="now-playing">
          <MediaPoster class="cover" :src="current.poster" :title="current.title" />
          <div class="meta">
            <span class="title">{{ current.title }}</span>
            <span v-if="current.subtitle" class="subtitle">{{ current.subtitle }}</span>
          </div>
        </div>

        <div class="controls">
          <button
            class="ctl"
            :disabled="!hasPrevious"
            aria-label="上一首"
            @click="player.playPrevious"
          >
            ⏮
          </button>
          <button class="ctl play" :aria-label="isPlaying ? '暂停' : '播放'" @click="togglePlay">
            {{ isPlaying ? "⏸" : "▶" }}
          </button>
          <button class="ctl" :disabled="!hasNext" aria-label="下一首" @click="player.playNext">
            ⏭
          </button>
        </div>

        <div class="progress">
          <span class="time">{{ formatDuration(Math.floor(currentTime)) || "0:00" }}</span>
          <input
            class="bar"
            type="range"
            min="0"
            :max="duration || 0"
            :value="currentTime"
            :style="{ '--progress': `${progress * 100}%` }"
            aria-label="播放进度"
            @input="onSeek"
          />
          <span class="time">{{ formatDuration(Math.floor(duration)) || "0:00" }}</span>
        </div>

        <button class="ctl close" aria-label="关闭播放栏" @click="closeBar">✕</button>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped lang="scss">
.music-bar {
  position: fixed;
  inset: auto 0 0 0;
  z-index: 60;
  display: grid;
  grid-template-columns: minmax(160px, 1fr) auto minmax(200px, 2fr) auto;
  align-items: center;
  gap: var(--fbz-space-5);
  padding: var(--fbz-space-3) var(--fbz-space-6);
  background: var(--fbz-color-panel-strong);
  border-top: 1px solid var(--fbz-color-line-soft);
}

.now-playing {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-3);
  min-width: 0;

  .cover {
    width: 44px;
    flex: none;
  }

  .meta {
    display: flex;
    flex-direction: column;
    min-width: 0;

    .title {
      font-size: var(--fbz-font-size-sm);
      font-weight: 600;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .subtitle {
      font-size: var(--fbz-font-size-xs);
      color: var(--fbz-color-text-muted);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
  }
}

.controls {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-2);
}

.ctl {
  display: grid;
  place-content: center;
  width: 36px;
  height: 36px;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-md);
  cursor: pointer;
  transition:
    background var(--fbz-motion-fast),
    color var(--fbz-motion-fast);

  &:hover:not(:disabled) {
    background: var(--fbz-color-panel);
  }

  &:disabled {
    color: var(--fbz-color-text-muted);
    cursor: default;
    opacity: 0.4;
  }

  &.play {
    background: var(--fbz-color-brand-500);
    color: #0a0a0b;

    &:hover {
      background: var(--fbz-color-brand-500);
      filter: brightness(1.1);
    }
  }
}

.progress {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-3);

  .time {
    font-size: var(--fbz-font-size-xs);
    color: var(--fbz-color-text-muted);
    font-variant-numeric: tabular-nums;
    min-width: 4ch;
    text-align: center;
  }

  .bar {
    flex: 1;
    appearance: none;
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(
      to right,
      var(--fbz-color-brand-500) var(--progress, 0%),
      var(--fbz-color-line-soft) var(--progress, 0%)
    );
    cursor: pointer;

    &::-webkit-slider-thumb {
      appearance: none;
      width: 12px;
      height: 12px;
      border-radius: 50%;
      background: var(--fbz-color-brand-500);
    }

    &::-moz-range-thumb {
      width: 12px;
      height: 12px;
      border: none;
      border-radius: 50%;
      background: var(--fbz-color-brand-500);
    }
  }
}

.music-bar-slide-enter-active,
.music-bar-slide-leave-active {
  transition: transform var(--fbz-motion-base);
}

.music-bar-slide-enter-from,
.music-bar-slide-leave-to {
  transform: translateY(100%);
}

@media (max-width: 600px) {
  .music-bar {
    grid-template-columns: 1fr auto;
    grid-template-rows: auto auto;
    gap: var(--fbz-space-3);
    padding: var(--fbz-space-3) var(--fbz-space-4);
  }

  .progress {
    grid-column: 1 / -1;
    order: 3;
  }

  .close {
    display: none;
  }
}
</style>
