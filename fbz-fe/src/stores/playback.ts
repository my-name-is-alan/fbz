import { preparePlayback } from "@/service/modules/server.ts";
import type { PlaybackMediaSourceOption } from "@/service/modules/server.ts";
import { useUiStore } from "@/stores/ui.ts";
export interface PlaybackTrack {
  id: string;
  label: string;
  language?: string;
  active?: boolean;
  /** PlaybackInfo 外挂字幕 DeliveryUrl（可相对站点根）；有值则走外挂挂轨。 */
  deliveryUrl?: string;
  codec?: string;
  isExternal?: boolean;
}

export interface PlaybackChapter {
  id: string;
  title: string;
  /** 章节起点（秒）。 */
  startTime: number;
  duration: number;
  /**
   * Emby MarkerType：IntroStart / IntroEnd / CreditsStart / CreditsEnd / Chapter 等。
   * 播放器据此显示「跳过片头/片尾」。
   */
  markerType?: string;
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
  /** 显式起播秒数；含 0（从头）。未设时回退 server.startTicks。 */
  startPositionSeconds?: number;
  source?: PlaybackSource;
  /** PlaybackInfo 多 MediaSource 版本列表（可切换）。 */
  mediaSources?: PlaybackMediaSourceOption[];
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
  const mediaSources = computed(() => item.value?.mediaSources ?? []);
  const activeMediaSourceId = computed(() => item.value?.server?.mediaSourceId);

  function open(nextItem: PlaybackItem) {
    selection++;
    item.value = nextItem;
  }

  async function selectEpisode(episodeId: string) {
    const version = ++selection;
    const current = item.value;
    const episode = playlist.value.find((entry) => entry.id === episodeId);
    if (!current || !episode) return;

    {
      try {
        const prepared = await preparePlayback(
          episode.serverItem ?? { Id: episode.id, Name: episode.title, Type: "Episode" },
        );
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
  }

  /** 切换同一条目的 MediaSource（版本）；保留当前进度由播放器传入 resumeSeconds。 */
  async function switchMediaSource(mediaSourceId: string, resumeSeconds?: number) {
    const version = ++selection;
    const current = item.value;
    if (!current || current.server?.mediaSourceId === mediaSourceId) return;
    if (!(current.mediaSources?.some((s) => s.id === mediaSourceId) ?? false)) return;

    try {
      const type =
        current.type === "episode" ? "Episode" : current.type === "tv" ? "Series" : "Movie";
      const prepared = await preparePlayback(
        { Id: current.id, Name: current.title, Type: type },
        mediaSourceId,
      );
      if (version !== selection || !item.value) return;
      item.value = {
        ...current,
        ...prepared,
        title: current.title,
        subtitle: current.subtitle,
        poster: current.poster ?? prepared.poster,
        backdrop: current.backdrop ?? prepared.backdrop,
        tags: current.tags,
        playlist: current.playlist,
        startPositionSeconds: resumeSeconds ?? current.startPositionSeconds ?? 0,
        mediaSources: prepared.mediaSources?.length ? prepared.mediaSources : current.mediaSources,
        chapters: prepared.chapters?.length ? prepared.chapters : current.chapters,
        subtitleTracks: prepared.subtitleTracks?.length
          ? prepared.subtitleTracks
          : current.subtitleTracks,
      };
    } catch {
      if (version !== selection) return;
      useUiStore().showToast("无法切换该版本，请稍后重试", "error");
    }
  }

  function playPreviousEpisode() {
    if (!hasPreviousEpisode.value) return;
    void selectEpisode(playlist.value[currentEpisodeIndex.value - 1]!.id);
  }

  /** 播放器内下一集：走当前剧集 playlist（同季/已装载列表）。跨剧「下一集待看」用 Shows/NextUp（首页继续观看），不在此调用。 */
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
    mediaSources,
    activeMediaSourceId,
    open,
    selectEpisode,
    switchMediaSource,
    playPreviousEpisode,
    playNextEpisode,
    close,
  };
});
