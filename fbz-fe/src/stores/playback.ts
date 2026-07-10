import { fetchPlaybackSource } from "@/service/modules/detail.ts";

export interface PlaybackTrack {
  id: string;
  label: string;
  language?: string;
  active?: boolean;
}

export interface PlaybackChapter {
  id: string;
  title: string;
  startTime: number;
  duration: number;
}

export interface PlaybackSource {
  uri?: string;
  mimeType?: string;
}

export interface PlaybackEpisode {
  id: string;
  title: string;
  subtitle?: string;
  seasonNumber: number;
  episodeNumber: number;
  duration: number;
  poster?: string;
  backdrop?: string;
}

export interface PlaybackItem {
  type: "movie" | "tv" | "episode";
  id: string;
  title: string;
  subtitle?: string;
  poster?: string;
  backdrop?: string;
  tags?: string[];
  duration?: number;
  /** 续播起点（秒）：播放器加载完媒体后 seek 到该位置。 */
  startPositionSeconds?: number;
  source?: PlaybackSource;
  chapters?: PlaybackChapter[];
  audioTracks?: PlaybackTrack[];
  subtitleTracks?: PlaybackTrack[];
  playlist?: PlaybackEpisode[];
}

export const usePlaybackStore = defineStore("playback", () => {
  const item = shallowRef<PlaybackItem>();
  const isOpen = computed(() => item.value != null);
  const playlist = computed(() => item.value?.playlist ?? []);
  const currentEpisodeIndex = computed(() => {
    if (!item.value || !playlist.value.length) return -1;
    return playlist.value.findIndex((episode) => episode.id === item.value?.id);
  });
  const hasPreviousEpisode = computed(() => currentEpisodeIndex.value > 0);
  const hasNextEpisode = computed(
    () => currentEpisodeIndex.value >= 0 && currentEpisodeIndex.value < playlist.value.length - 1,
  );

  function open(nextItem: PlaybackItem) {
    item.value = nextItem;
  }

  async function selectEpisode(episodeId: string) {
    const current = item.value;
    const episode = playlist.value.find((entry) => entry.id === episodeId);
    if (!current || !episode) return;

    // 先切到目标集（清空上一集的 source 与续播起点，避免闪播旧流/错位 seek），
    // 再补拉该集真实流地址。
    item.value = {
      ...current,
      type: "episode",
      id: episode.id,
      title: episode.title,
      subtitle: episode.subtitle,
      poster: episode.poster ?? current.poster,
      backdrop: episode.backdrop ?? current.backdrop,
      duration: episode.duration,
      startPositionSeconds: undefined,
      source: undefined,
    };

    const source = await fetchPlaybackSource(episode.id);
    // 拉取期间用户可能又切了集：仅当仍停留在该集时才写回 source。
    if (source && item.value?.id === episode.id) {
      item.value = { ...item.value, source: { uri: source.uri, mimeType: source.mimeType } };
    }
  }

  function playPreviousEpisode() {
    if (!hasPreviousEpisode.value) return;
    void selectEpisode(playlist.value[currentEpisodeIndex.value - 1]!.id);
  }

  function playNextEpisode() {
    if (!hasNextEpisode.value) return;
    void selectEpisode(playlist.value[currentEpisodeIndex.value + 1]!.id);
  }

  function close() {
    item.value = undefined;
  }

  return {
    item,
    isOpen,
    playlist,
    currentEpisodeIndex,
    hasPreviousEpisode,
    hasNextEpisode,
    open,
    selectEpisode,
    playPreviousEpisode,
    playNextEpisode,
    close,
  };
});
