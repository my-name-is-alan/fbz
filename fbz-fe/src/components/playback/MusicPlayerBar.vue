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
  // 续听：元数据就绪后跳到起播位置（有声书）。
  const startSeconds = current.value?.startSeconds ?? 0;
  if (startSeconds > 0) {
    el.addEventListener(
      "loadedmetadata",
      () => {
        if (Number.isFinite(el.duration) && startSeconds < el.duration)
          el.currentTime = startSeconds;
      },
      { once: true },
    );
  }

  // 新曲会话：记录 id + 新会话标识，上报 start（best-effort）。
  reportedItemId = current.value?.id;
  playSessionId = newPlaySessionId();
  lastProgressReportedAt = 0;
  if (reportedItemId) {
    void reportPlaybackStart({
      itemId: reportedItemId,
      playSessionId,
      positionSeconds: startSeconds,
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
      <div v-if="current" class="music-bar c-force-dark" role="region" aria-label="音乐播放栏">
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
          <div class="cover">
            <MediaPoster :src="current.poster" :title="current.title" ratio="square" icon="music" />
          </div>
          <div class="meta">
            <span class="title">{{ current.title }}</span>
            <span v-if="current.subtitle" class="subtitle">{{ current.subtitle }}</span>
          </div>
        </div>

        <div class="center">
          <div class="controls">
            <button
              class="ctl"
              type="button"
              :disabled="!hasPrevious"
              aria-label="上一首"
              @click="player.playPrevious"
            >
              <BaseIcon name="previous" :size="18" />
            </button>
            <button
              class="ctl play"
              type="button"
              :aria-label="isPlaying ? '暂停' : '播放'"
              @click="togglePlay"
            >
              <BaseIcon :name="isPlaying ? 'pause' : 'play'" :size="18" fill />
            </button>
            <button
              class="ctl"
              type="button"
              :disabled="!hasNext"
              aria-label="下一首"
              @click="player.playNext"
            >
              <BaseIcon name="next" :size="18" />
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
        </div>

        <button class="ctl close" type="button" aria-label="关闭播放栏" @click="closeBar">
          <BaseIcon name="close" :size="16" />
        </button>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped lang="scss">
.music-bar {
  position: fixed;
  left: 50%;
  bottom: 16px;
  z-index: 45;
  width: min(960px, calc(100vw - 32px));
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 10px 14px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  background: var(--ui-glass-strong);
  box-shadow: var(--ui-shadow-dialog);
  backdrop-filter: blur(20px);
  transform: translateX(-50%);
}

.now-playing {
  flex: 0 1 260px;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 10px;
}

.cover {
  flex: 0 0 auto;
  width: 44px;
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
}

.meta {
  min-width: 0;
  display: flex;
  flex-direction: column;

  .title,
  .subtitle {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .title {
    color: var(--ui-text);
    font-size: 13px;
    font-weight: 600;
  }

  .subtitle {
    color: var(--ui-text-muted);
    font-size: 11.5px;
  }
}

.center {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
}

.controls {
  display: flex;
  align-items: center;
  gap: 6px;
}

.ctl {
  width: 34px;
  height: 34px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  color: var(--ui-text);
  cursor: pointer;

  &:hover:not(:disabled) {
    background: var(--ui-surface-hover);
  }

  &:disabled {
    opacity: 0.35;
    cursor: default;
  }

  &.play {
    width: 38px;
    height: 38px;
    padding-left: 2px;
    border-radius: var(--ui-radius-md);
    background: var(--ui-accent);
    color: var(--ui-accent-contrast);

    &:hover {
      background: var(--ui-accent-hover);
    }
  }

  &.close {
    flex: 0 0 auto;
    color: var(--ui-text-muted);
  }
}

.progress {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 10px;
}

.time {
  flex: 0 0 auto;
  min-width: 38px;
  color: var(--ui-text-subtle);
  font-family: var(--ui-font-mono);
  font-size: 11px;
  text-align: center;
}

.bar {
  flex: 1;
  height: 14px;
  margin: 0;
  appearance: none;
  background: transparent;
  cursor: pointer;

  &::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 99px;
    background: linear-gradient(
      90deg,
      var(--ui-accent) 0 var(--progress, 0%),
      rgb(255 255 255 / 16%) var(--progress, 0%) 100%
    );
  }

  &::-webkit-slider-thumb {
    width: 12px;
    height: 12px;
    margin-top: -4px;
    appearance: none;
    border-radius: 50%;
    background: var(--ui-accent-hover);
  }

  &::-moz-range-track {
    height: 4px;
    border-radius: 99px;
    background: rgb(255 255 255 / 16%);
  }

  &::-moz-range-progress {
    height: 4px;
    border-radius: 99px;
    background: var(--ui-accent);
  }
}

.music-bar-slide-enter-active,
.music-bar-slide-leave-active {
  transition:
    transform var(--ui-duration-base) var(--ui-ease-out),
    opacity var(--ui-duration-base);
}

.music-bar-slide-enter-from,
.music-bar-slide-leave-to {
  opacity: 0;
  transform: translate(-50%, 16px);
}

@media (max-width: 600px) {
  .music-bar {
    bottom: 8px;
    gap: 8px;
    padding: 8px 10px;
  }

  .now-playing {
    flex-basis: 40%;
  }

  .progress .time {
    display: none;
  }
}
</style>
