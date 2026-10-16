import type { Ref } from "vue";
import { onBeforeUnmount, reactive, shallowRef, watch } from "vue";
import { useRouter } from "vue-router";
import { useEventListener } from "@vueuse/core";
import type { DetailViewModel } from "@/service/modules/detail.ts";
import { setFavorite, setPlayed } from "@/service/modules/userData.ts";
import { useAuthStore } from "@/stores/auth.ts";
import { useUiStore } from "@/stores/ui.ts";
import type { MediaItem } from "@/types/media.ts";

/**
 * 详情页公共动作：收藏 / 已看（乐观更新 + 失败回滚）、「更多」菜单（复用条目右键菜单：
 * 加入播放列表 / 合集、刷新元数据、识别、编辑、删除）、编辑元数据。
 */
export function useDetailActions(detail: Ref<DetailViewModel | undefined>, fallback: string) {
  const authStore = useAuthStore();
  const uiStore = useUiStore();
  const router = useRouter();
  const busy = shallowRef<"favorite" | "played" | null>(null);

  async function toggleFavorite() {
    const item = detail.value;
    const userId = authStore.userId;
    if (!item || !userId || busy.value) return;
    const next = !item.isFavorite;
    busy.value = "favorite";
    detail.value = { ...item, isFavorite: next };
    try {
      await setFavorite(userId, item.id, next);
    } catch {
      if (detail.value?.id === item.id) detail.value = { ...detail.value, isFavorite: !next };
      uiStore.showToast("更新收藏状态失败。", "error");
    } finally {
      busy.value = null;
    }
  }

  async function togglePlayed() {
    const item = detail.value;
    const userId = authStore.userId;
    if (!item || !userId || busy.value) return;
    const next = !item.played;
    busy.value = "played";
    detail.value = {
      ...item,
      played: next,
      resumePositionSeconds: next ? undefined : item.resumePositionSeconds,
    };
    try {
      await setPlayed(userId, item.id, next);
      window.dispatchEvent(new Event("fbz-playback-saved"));
    } catch {
      if (detail.value?.id === item.id) detail.value = { ...detail.value, played: !next };
      uiStore.showToast("更新观看状态失败。", "error");
    } finally {
      busy.value = null;
    }
  }

  /** 详情视图模型 → 右键菜单 / 元数据弹窗需要的最小 MediaItem。 */
  function asMediaItem(): MediaItem | undefined {
    const item = detail.value;
    if (!item) return undefined;
    const yearSeg = item.meta.find((m) => /^\d{4}$/.test(m));
    return reactive<MediaItem>({
      id: item.id,
      libraryId: item.detailType === "tv" ? "series" : "movie",
      detailType: item.detailType,
      itemType: item.itemType,
      title: item.title,
      meta: item.meta.join(" · "),
      poster: item.poster,
      year: yearSeg ? Number(yearSeg) : undefined,
      rating: item.rating ?? undefined,
      isFavorite: item.isFavorite,
      isPlayed: item.played,
    });
  }

  let menuItem: MediaItem | undefined;
  let stopSync: (() => void) | undefined;

  /** 打开条目菜单；菜单内改动（收藏 / 已看）回写详情视图模型。 */
  function openMore(event: MouseEvent) {
    const item = asMediaItem();
    if (!item) return;
    stopSync?.();
    menuItem = item;
    stopSync = watch(
      () => [menuItem?.isFavorite, menuItem?.isPlayed] as const,
      ([fav, played]) => {
        if (!detail.value || detail.value.id !== menuItem?.id) return;
        detail.value = { ...detail.value, isFavorite: Boolean(fav), played: Boolean(played) };
      },
    );
    const rect = (event.currentTarget as HTMLElement | null)?.getBoundingClientRect();
    uiStore.openContextMenu(
      rect ? Math.min(rect.left, window.innerWidth - 240) : event.clientX,
      rect ? rect.bottom + 6 : event.clientY,
      item,
    );
  }

  function editMetadata() {
    const item = asMediaItem();
    if (item) uiStore.openMetadataManager(item);
  }

  // 右键菜单删除了当前条目 → 离开详情页。
  useEventListener(window, "fbz:item-deleted", ((event: CustomEvent<{ id: string }>) => {
    if (event.detail?.id && event.detail.id === detail.value?.id) {
      if (window.history.state?.back) router.back();
      else void router.push(fallback);
    }
  }) as EventListener);

  onBeforeUnmount(() => stopSync?.());

  return { busy, toggleFavorite, togglePlayed, openMore, editMetadata, asMediaItem };
}
