import { preparePlayback } from "@/service/modules/server.ts";
import { useUiStore } from "@/stores/ui.ts";
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
  proxyUri?: string;
  size?: number;
  uri?: string;
  mimeType?: string;
}

export interface PlaybackEpisode {
  serverItem?: import("@/service/modules/server.ts").ServerItem;
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
  server?: { playSessionId: string; mediaSourceId: string; startTicks: number };
  type: "movie" | "tv" | "episode";
  id: string;
  title: string;
  subtitle?: string;
  poster?: string;
  backdrop?: string;
  tags?: string[];
  duration?: number;
  source?: PlaybackSource;
  chapters?: PlaybackChapter[];
  audioTracks?: PlaybackTrack[];
  subtitleTracks?: PlaybackTrack[];
  playlist?: PlaybackEpisode[];
}

export const usePlaybackStore = defineStore("playback", () => {
  const item = shallowRef<PlaybackItem>();
  let selection = 0;
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
    selection++;
    item.value = nextItem;
  }

  async function selectEpisode(episodeId: string) {
    const version = ++selection;
    const current = item.value;
    const episode = playlist.value.find((entry) => entry.id === episodeId);
    if (!current || !episode) return;

    if (episode.serverItem) {
      try {
        const prepared = await preparePlayback(episode.serverItem);
        if (version !== selection || !item.value) return;
        item.value = {
          ...prepared,
          title: episode.title,
          subtitle: episode.subtitle,
          backdrop: episode.backdrop,
          playlist: current.playlist,
        };
      } catch {
        if (version !== selection) return;
        useUiStore().showToast("无法加载分集，请稍后重试", "error");
      }
      return;
    }
    item.value = {
      ...current,
      type: "episode",
      id: episode.id,
      title: episode.title,
      subtitle: episode.subtitle,
      poster: episode.poster ?? current.poster,
      backdrop: episode.backdrop ?? current.backdrop,
      duration: episode.duration,
    };
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
    selection++;
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
