<script setup lang="ts">
import type { MediaVersion } from "@/types/media.ts";

/**
 * 详情页头部 —— Emby 式 poster + fanart 布局。
 * fanart（剧照）铺满顶部作为背景，poster（海报）叠在左侧，元信息在右侧。
 * 支持：多版本切换（播放时带出所选版本 id）、收藏切换、续播（从上次位置继续）。
 */
interface Props {
  title: string;
  poster?: string;
  backdrop?: string;
  /** 标题下方的信息片段，如 ["2024", "2h 6m"] */
  meta?: string[];
  /** 题材名列表，渲染为 chips 行 */
  genres?: string[];
  /** 分级（如 PG-13），显示为边框徽章 */
  officialRating?: string;
  overview?: string;
  rating?: number | null;
  /** 是否显示播放按钮（人物页不需要） */
  showActions?: boolean;
  /** 可选播放版本；多版本时显示下拉 */
  versions?: MediaVersion[];
  /** 收藏态（点亮书签图标） */
  isFavorite?: boolean;
  /** 续播位置（秒）；有值时主按钮变「继续播放」并附「从头播放」 */
  resumePositionSeconds?: number;
  /** 已看完（显示徽标） */
  played?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  meta: () => [],
  genres: () => [],
  showActions: true,
  versions: () => [],
  isFavorite: false,
  played: false,
});
const emit = defineEmits<{
  /** 播放（从头）。载荷 = 所选版本 id（无版本信息时 undefined）。 */
  play: [versionId?: string];
  /** 续播（从 resumePositionSeconds 继续）。 */
  resume: [versionId?: string];
  /** 收藏/取消收藏。 */
  toggleFavorite: [];
}>();

const activeVersionId = ref(props.versions[0]?.id ?? "");
watch(
  () => props.versions,
  (v) => {
    activeVersionId.value = v[0]?.id ?? "";
  },
);

const activeVersion = computed(
  () => props.versions.find((v) => v.id === activeVersionId.value) ?? props.versions[0],
);

const versionOptions = computed(() => props.versions.map((v) => ({ label: v.label, value: v.id })));

const resumeLabel = computed(() => {
  const seconds = props.resumePositionSeconds;
  if (!seconds) return "";
  const totalMinutes = Math.floor(seconds / 60);
  const h = Math.floor(totalMinutes / 60);
  const m = totalMinutes % 60;
  const s = Math.floor(seconds % 60);
  const position = h
    ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
    : `${m}:${String(s).padStart(2, "0")}`;
  return `从 ${position} 继续`;
});

function versionIdOrUndefined(): string | undefined {
  return activeVersion.value?.id;
}
</script>

<template>
  <header class="detail-hero">
    <div class="backdrop">
      <img v-if="props.backdrop" :src="props.backdrop" :alt="props.title" />
      <div class="backdrop-scrim" />
    </div>

    <div class="hero-body">
      <div class="poster">
        <MediaPoster :src="props.poster" :title="props.title" ratio="poster" />
      </div>

      <div class="info">
        <h1 class="title">{{ props.title }}</h1>

        <div v-if="props.meta.length || props.rating != null || props.officialRating" class="meta">
          <span v-if="props.rating != null" class="rating">★ {{ props.rating.toFixed(1) }}</span>
          <span v-if="props.officialRating" class="cert">{{ props.officialRating }}</span>
          <template v-for="(m, i) in props.meta" :key="i">
            <span class="dot" />
            <span>{{ m }}</span>
          </template>
          <span v-if="props.played" class="played-badge">已看完</span>
        </div>

        <div v-if="props.genres.length" class="genres">
          <span v-for="genre in props.genres" :key="genre" class="genre-chip">{{ genre }}</span>
        </div>

        <!-- 版本规格 tag（随所选版本同步） -->
        <div v-if="activeVersion" class="tags">
          <span v-for="t in activeVersion.tags" :key="t" class="tag">{{ t }}</span>
        </div>
        <div v-if="activeVersion?.subtitles.length" class="subs">
          <span class="subs-label">字幕</span>
          <span>{{ activeVersion.subtitles.join(" / ") }}</span>
        </div>

        <p v-if="props.overview" class="overview">{{ props.overview }}</p>

        <div v-if="props.showActions" class="actions">
          <!-- 有续播位置：主按钮 = 续播，附「从头播放」次按钮 -->
          <button
            v-if="props.resumePositionSeconds"
            class="btn btn-play"
            type="button"
            @click="emit('resume', versionIdOrUndefined())"
          >
            <svg class="btn-icon" viewBox="0 0 24 24" width="18" height="18" fill="currentColor">
              <path d="M8 5v14l11-7z" />
            </svg>
            <span>{{ resumeLabel }}</span>
          </button>
          <button
            v-if="props.resumePositionSeconds"
            class="btn btn-ghost"
            type="button"
            @click="emit('play', versionIdOrUndefined())"
          >
            <span>从头播放</span>
          </button>

          <button
            v-else
            class="btn btn-play"
            type="button"
            @click="emit('play', versionIdOrUndefined())"
          >
            <svg class="btn-icon" viewBox="0 0 24 24" width="18" height="18" fill="currentColor">
              <path d="M8 5v14l11-7z" />
            </svg>
            <span>播放</span>
          </button>

          <!-- 多版本下拉 -->
          <BaseSelect
            v-if="props.versions.length > 1"
            v-model="activeVersionId"
            :options="versionOptions"
            size="md"
            aria-label="选择版本"
            class="version-select"
          />
          <span v-else-if="activeVersion" class="version-single">{{ activeVersion.label }}</span>

          <button
            class="btn btn-ghost fav-btn"
            :class="{ active: props.isFavorite }"
            type="button"
            :aria-pressed="props.isFavorite"
            @click="emit('toggleFavorite')"
          >
            <svg
              class="btn-icon"
              viewBox="0 0 24 24"
              width="18"
              height="18"
              :fill="props.isFavorite ? 'currentColor' : 'none'"
              stroke="currentColor"
              stroke-width="2.2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" />
            </svg>
            <span>{{ props.isFavorite ? "已收藏" : "收藏" }}</span>
          </button>
        </div>

        <div class="extra">
          <slot name="extra" />
        </div>
      </div>
    </div>
  </header>
</template>

<style scoped lang="scss">
.detail-hero {
  position: relative;
}

.backdrop {
  position: absolute;
  inset: 0 0 auto 0;
  height: 62vh;
  min-height: 440px;
  max-height: 100%;
  z-index: 0;

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center 20%;
  }
}

.backdrop-scrim {
  position: absolute;
  inset: 0;
  background:
    linear-gradient(90deg, rgba(0, 0, 0, 0.85) 0%, rgba(0, 0, 0, 0.4) 45%, rgba(0, 0, 0, 0.1) 75%),
    linear-gradient(0deg, var(--fbz-color-bg) 1%, rgba(0, 0, 0, 0.25) 38%, rgba(0, 0, 0, 0.45) 100%);
}

.hero-body {
  position: relative;
  z-index: 1;
  display: flex;
  align-items: flex-end;
  gap: var(--fbz-space-8);
  max-width: 1280px;
  margin: 0 auto;
  // 顶部留出 header 高度即可；用 min-height 让内容稳定贴在 backdrop 底部，
  // 不再用大比例 vh 顶部内边距（缩放/短视口时会导致海报诡异地底部对齐）
  min-height: calc(62vh - var(--fbz-space-8));
  padding: calc(var(--header-h, 60px) + var(--fbz-space-6)) var(--fbz-space-8) var(--fbz-space-8);
}

.poster {
  flex: 0 0 232px;
  width: 232px;
  align-self: flex-end;
  border-radius: var(--fbz-radius-hero);
  overflow: hidden;
  border: 1px solid var(--fbz-color-line);
  box-shadow: var(--fbz-shadow-panel);
}

.info {
  flex: 1;
  min-width: 0;
  padding-bottom: var(--fbz-space-2);
  color: #ffffff; /* 确保在亮色主题下，叠加在剧照大图上的文字依然为白色，以保证最佳可读性 */
}

.title {
  margin: 0 0 var(--fbz-space-3);
  font-size: 40px;
  line-height: 1.1;
  font-weight: 800;
  color: #ffffff;
}

.meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--fbz-space-3);
  margin-bottom: var(--fbz-space-3);
  font-size: var(--fbz-font-size-md);
  color: rgba(255, 255, 255, 0.85);

  .dot {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.4);
  }

  .rating {
    color: var(--fbz-color-brand-500);
    font-weight: 700;
  }

  .cert {
    padding: 1px 7px;
    border: 1px solid rgba(255, 255, 255, 0.35);
    border-radius: 3px;
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
    letter-spacing: 0.5px;
  }

  .played-badge {
    padding: 1px 8px;
    border-radius: 3px;
    background: color-mix(in srgb, var(--fbz-color-brand-500) 16%, transparent);
    color: var(--fbz-color-brand-500);
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
  }
}

.genres {
  display: flex;
  flex-wrap: wrap;
  gap: var(--fbz-space-2);
  margin-bottom: var(--fbz-space-3);
}

.genre-chip {
  padding: 3px 11px;
  border-radius: 999px;
  border: 1px solid rgba(255, 255, 255, 0.18);
  background: rgba(255, 255, 255, 0.06);
  font-size: var(--fbz-font-size-xs);
  font-weight: 600;
  color: rgba(255, 255, 255, 0.85);
}

.tags {
  display: flex;
  flex-wrap: wrap;
  gap: var(--fbz-space-2);
  margin-bottom: var(--fbz-space-3);
}

.tag {
  padding: 3px 9px;
  border: 1px solid rgba(255, 255, 255, 0.22);
  background: rgba(255, 255, 255, 0.05);
  border-radius: 3px;
  font-size: var(--fbz-font-size-xs);
  font-weight: 600;
  letter-spacing: 0.4px;
  color: rgba(255, 255, 255, 0.85);
}

.subs {
  display: flex;
  gap: var(--fbz-space-2);
  margin-bottom: var(--fbz-space-4);
  font-size: var(--fbz-font-size-sm);
  color: rgba(255, 255, 255, 0.85);

  .subs-label {
    color: rgba(255, 255, 255, 0.5);
  }
}

.overview {
  max-width: 720px;
  margin: 0 0 var(--fbz-space-5);
  font-size: var(--fbz-font-size-md);
  line-height: 1.7;
  color: rgba(255, 255, 255, 0.8);
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 4;
  line-clamp: 4;
  overflow: hidden;
}

.actions {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--fbz-space-3);
  margin-bottom: var(--fbz-space-5);
}

.btn {
  height: 44px;
  padding: 0 22px;
  border: 1px solid transparent;
  border-radius: var(--fbz-radius-control);
  font-size: var(--fbz-font-size-md);
  font-weight: 700;
  text-decoration: none;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  transition:
    background var(--fbz-motion-fast) ease,
    border-color var(--fbz-motion-fast) ease,
    box-shadow var(--fbz-motion-fast) ease,
    transform var(--fbz-motion-fast) ease;

  &:active {
    transform: scale(0.96);
  }
}

.btn-icon {
  flex: 0 0 auto;
}

.btn-play {
  color: #07120a;
  background: var(--fbz-color-brand-500);

  &:hover {
    background: var(--fbz-color-brand-600);
    box-shadow: 0 6px 20px color-mix(in srgb, var(--fbz-color-brand-500) 30%, transparent);
    transform: translateY(-2px);
  }
}

.btn-ghost {
  color: #ffffff;
  background: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 255, 255, 0.16);

  &:hover {
    background: rgba(255, 255, 255, 0.18);
    border-color: rgba(255, 255, 255, 0.35);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.25);
    transform: translateY(-2px);
  }
}

.fav-btn.active {
  color: var(--fbz-color-brand-500);
  border-color: color-mix(in srgb, var(--fbz-color-brand-500) 45%, transparent);
  background: color-mix(in srgb, var(--fbz-color-brand-500) 10%, transparent);
}

.version-single {
  font-size: var(--fbz-font-size-sm);
  color: rgba(255, 255, 255, 0.5);
}

.extra :deep(.fact) {
  dt {
    color: rgba(255, 255, 255, 0.5) !important;
  }
  dd {
    color: rgba(255, 255, 255, 0.85) !important;
  }
  .link {
    color: var(--fbz-color-brand-500) !important;
  }
}

@media (max-width: 1024px) {
  .poster {
    flex-basis: 180px;
    width: 180px;
  }

  .title {
    font-size: 32px;
  }
}

@media (max-width: 600px) {
  .backdrop {
    height: 42vh;
    min-height: 280px;
  }

  .hero-body {
    flex-direction: column;
    align-items: stretch;
    gap: var(--fbz-space-4);
    min-height: 0;
    padding: calc(var(--header-h, 56px) + 30vw) var(--fbz-space-4) var(--fbz-space-5);
  }

  .poster {
    flex-basis: auto;
    width: 120px;
    align-self: flex-start;
  }

  .title {
    font-size: 26px;
  }

  .actions .btn {
    flex: 1;
    justify-content: center;
  }

  .version-select {
    flex: 1 0 100%;
    display: block;
  }
}
</style>
