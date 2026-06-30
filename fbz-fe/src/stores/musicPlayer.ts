import { audioStreamUrl } from "@/service/request.ts";

/** 播放队列里的一首曲目（音乐播放栏所需的最小信息）。 */
export interface PlayerTrack {
  id: string;
  title: string;
  /** 所属专辑/艺术家副标题，展示用。 */
  subtitle?: string;
  /** 封面绝对地址；为空时播放栏渲染占位块。 */
  poster?: string;
}

/**
 * 音乐播放 store —— 独立于视频 [`usePlaybackStore`]（后者走 shaka 全屏播放器）。
 * 音乐用底部播放栏 + 原生 `<audio>`：直出流支持 RANGE，无需 shaka/HLS。
 * 维护一个队列 + 当前索引，支持上一首/下一首；曲目流地址由 {@link audioStreamUrl} 拼。
 */
export const useMusicPlayerStore = defineStore("musicPlayer", () => {
  const queue = ref<PlayerTrack[]>([]);
  const currentIndex = ref(-1);

  const current = computed<PlayerTrack | undefined>(() => queue.value[currentIndex.value]);
  const isActive = computed(() => current.value != null);
  const hasPrevious = computed(() => currentIndex.value > 0);
  const hasNext = computed(
    () => currentIndex.value >= 0 && currentIndex.value < queue.value.length - 1,
  );

  /** 当前曲目的直出流地址；无曲目或未登录时为 undefined。 */
  const streamUrl = computed(() => (current.value ? audioStreamUrl(current.value.id) : undefined));

  /** 用一个曲目列表替换队列，并从 `startIndex` 处开始播放（默认首曲）。 */
  function playQueue(tracks: PlayerTrack[], startIndex = 0) {
    if (!tracks.length) return;
    queue.value = tracks;
    currentIndex.value = Math.min(Math.max(startIndex, 0), tracks.length - 1);
  }

  function playPrevious() {
    if (hasPrevious.value) currentIndex.value -= 1;
  }

  function playNext() {
    if (hasNext.value) currentIndex.value += 1;
  }

  function close() {
    queue.value = [];
    currentIndex.value = -1;
  }

  return {
    queue,
    currentIndex,
    current,
    isActive,
    hasPrevious,
    hasNext,
    streamUrl,
    playQueue,
    playPrevious,
    playNext,
    close,
  };
});
