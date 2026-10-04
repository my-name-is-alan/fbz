<script setup lang="ts">
/**
 * 家庭库（homevideos）图片时间线 —— 消费 fbz-api 的
 * `GET /api/admin/libraries/{id}/photos`（按拍摄时间倒序，keyset 翻页）与
 * `GET /api/admin/media-items/{id}/thumbnail`（鉴权缩略图，走 blob URL）。
 *
 * 缩略图端点需 x-emby-token，<img> 不会自动带头，故用 service 取 blob 再转
 * object URL；离开/切库时统一 revoke 释放，避免内存泄漏。
 * 后端不可达（设计态离线预览）时静默空态，不打扰用户。
 */
import type { LibraryPhoto, LibrarySettings } from "@/types/admin.ts";
import { fetchPhotoThumbnail, listLibraries, listLibraryPhotos } from "@/service/modules/admin.ts";

const PAGE_SIZE = 60;

const homeLibraries = ref<LibrarySettings[]>([]);
const selectedLibraryId = ref<string>("");
const photos = ref<LibraryPhoto[]>([]);
const nextCursor = ref<string | null>(null);
const hasMore = ref(false);
const loading = ref(false);
const loadError = ref(false);
/** itemId → 缩略图 blob URL，统一在此持有以便 revoke。 */
const thumbnails = ref<Record<string, string>>({});

const libraryOptions = computed(() =>
  homeLibraries.value.map((lib) => ({ label: lib.name, value: lib.id })),
);

/** 释放所有已生成的 blob URL（切库/卸载时调用，防内存泄漏）。 */
function revokeThumbnails() {
  for (const url of Object.values(thumbnails.value)) {
    URL.revokeObjectURL(url);
  }
  thumbnails.value = {};
}

/** 为一页照片懒加载缩略图（逐张取 blob，失败的留空走占位）。 */
async function loadThumbnails(items: LibraryPhoto[]) {
  await Promise.all(
    items
      .filter((photo) => photo.hasThumbnail && !thumbnails.value[photo.id])
      .map(async (photo) => {
        try {
          thumbnails.value[photo.id] = await fetchPhotoThumbnail(photo.id);
        } catch {
          // 缩略图尚未生成（404）或后端不可达：留空，模板渲染占位块。
        }
      }),
  );
}

/** 拉取一页照片并追加；reset=true 时换库重来。 */
async function loadPhotos(reset = false) {
  if (loading.value || !selectedLibraryId.value) return;
  loading.value = true;
  loadError.value = false;
  try {
    const page = await listLibraryPhotos(selectedLibraryId.value, {
      limit: PAGE_SIZE,
      cursor: reset ? undefined : (nextCursor.value ?? undefined),
    });
    if (reset) {
      revokeThumbnails();
      photos.value = page.items;
    } else {
      photos.value = [...photos.value, ...page.items];
    }
    hasMore.value = page.hasMore;
    nextCursor.value = page.nextCursor;
    void loadThumbnails(page.items);
  } catch {
    loadError.value = true;
  } finally {
    loading.value = false;
  }
}

/** 切库：清空当前时间线并重新拉取。 */
watch(selectedLibraryId, () => {
  photos.value = [];
  nextCursor.value = null;
  hasMore.value = false;
  void loadPhotos(true);
});

onMounted(async () => {
  try {
    // 时间线只对家庭库有意义，拉全部库后按 homevideos 过滤。
    const page = await listLibraries({ libraryType: "homevideos", limit: 200 });
    homeLibraries.value = page.items;
    if (page.items.length > 0) {
      selectedLibraryId.value = page.items[0].id;
    }
  } catch {
    // 后端未连接（设计态预览）：保留空态。
    loadError.value = true;
  }
});

onBeforeUnmount(revokeThumbnails);

/** 拍摄时间格式化为「YYYY-MM-DD HH:mm」展示（后端给的是 ISO 文本）。 */
function formatCaptured(value: string | null): string {
  if (!value) return "未知时间";
  return value.replace("T", " ").slice(0, 16);
}

/** 拼一行简洁的 EXIF 摘要（相机 · f值 · 快门 · ISO），缺失项跳过。 */
function exifSummary(photo: LibraryPhoto): string {
  const parts: string[] = [];
  if (photo.cameraModel) parts.push(photo.cameraModel);
  if (photo.fNumber) parts.push(`f/${photo.fNumber}`);
  if (photo.exposureTime) parts.push(`${photo.exposureTime}s`);
  if (photo.iso) parts.push(`ISO ${photo.iso}`);
  return parts.join(" · ");
}
</script>

<template>
  <div class="admin-photos">
    <div v-if="homeLibraries.length > 1" class="photos-toolbar">
      <span class="toolbar-label">家庭库</span>
      <BaseSelect
        v-model="selectedLibraryId"
        :options="libraryOptions"
        size="sm"
        aria-label="选择家庭库"
      />
    </div>

    <p v-if="loadError && photos.length === 0" class="photos-empty">
      暂无法加载图片时间线（后端未连接或该库尚无已识别照片）。
    </p>
    <p
      v-else-if="!loading && photos.length === 0 && homeLibraries.length === 0"
      class="photos-empty"
    >
      还没有家庭库（homevideos）。在「媒体库管理」里创建一个家庭库并扫描照片后，时间线会显示在这里。
    </p>
    <p v-else-if="!loading && photos.length === 0" class="photos-empty">
      该库还没有已提取元数据的照片。扫描完成、图片提取任务跑完后会出现在这里。
    </p>

    <div v-if="photos.length > 0" class="photo-grid">
      <figure v-for="photo in photos" :key="photo.id" class="photo-cell">
        <div class="photo-thumb">
          <img
            v-if="thumbnails[photo.id]"
            :src="thumbnails[photo.id]"
            :alt="photo.title"
            loading="lazy"
          />
          <div v-else class="photo-placeholder" aria-hidden="true" />
        </div>
        <figcaption class="photo-meta">
          <span class="photo-time">{{ formatCaptured(photo.capturedAt) }}</span>
          <span v-if="exifSummary(photo)" class="photo-exif">{{ exifSummary(photo) }}</span>
        </figcaption>
      </figure>
    </div>

    <div v-if="hasMore" class="photos-more">
      <button type="button" class="more-btn" :disabled="loading" @click="loadPhotos(false)">
        {{ loading ? "加载中…" : "加载更多" }}
      </button>
    </div>
  </div>
</template>

<style scoped lang="scss">
.admin-photos {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.photos-toolbar {
  display: flex;
  align-items: center;
  gap: 10px;

  .toolbar-label {
    font-size: 13px;
    color: var(--fbz-color-text-secondary, #9a9a9c);
  }
}

.photos-empty {
  padding: 40px 16px;
  text-align: center;
  font-size: 14px;
  color: var(--fbz-color-text-secondary, #9a9a9c);
}

.photo-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  gap: 12px;
}

.photo-cell {
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.photo-thumb {
  aspect-ratio: 1 / 1;
  border-radius: 4px;
  overflow: hidden;
  background: var(--fbz-color-surface-2, #161618);

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
}

.photo-placeholder {
  width: 100%;
  height: 100%;
  background: linear-gradient(135deg, #1a1a1c 0%, #232325 100%);
}

.photo-meta {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.photo-time {
  font-size: 12px;
  color: var(--fbz-color-text, #e8e8ea);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.photo-exif {
  font-size: 11px;
  color: var(--fbz-color-text-secondary, #9a9a9c);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.photos-more {
  display: flex;
  justify-content: center;
  padding: 8px 0 24px;
}

.more-btn {
  padding: 8px 24px;
  border: 1px solid var(--fbz-color-border, #2a2a2c);
  border-radius: 6px;
  background: transparent;
  color: var(--fbz-color-text, #e8e8ea);
  font-size: 13px;
  cursor: pointer;
  transition: border-color 0.15s ease;

  &:hover:not(:disabled) {
    border-color: var(--fbz-color-brand-500, #1ed760);
  }

  &:disabled {
    opacity: 0.5;
    cursor: default;
  }
}
</style>
