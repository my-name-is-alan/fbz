<script setup lang="ts">
import type { MediaVersion } from "@/types/media.ts";

/**
 * 详情页头部 —— Emby 式 poster + fanart 布局。
 * fanart（剧照）铺满顶部作为背景，poster（海报）叠在左侧，元信息在右侧。
 * 支持多播放版本：下拉切换版本，规格 tag 与字幕随之同步。
 */
interface Props {
  title: string;
  poster?: string;
  backdrop?: string;
  /** 标题下方的信息片段，如 ["2024", "166 min", "科幻"] */
  meta?: string[];
  tagline?: string;
  overview?: string;
  rating?: number | null;
  /** 是否显示播放按钮（人物页不需要） */
  showActions?: boolean;
  /** 可选播放版本；多版本时显示下拉 */
  versions?: MediaVersion[];
}

const props = withDefaults(defineProps<Props>(), {
  meta: () => [],
  showActions: true,
  versions: () => [],
});
const emit = defineEmits<{
  play: [];
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
        <p v-if="props.tagline" class="tagline">{{ props.tagline }}</p>

        <div v-if="props.meta.length || props.rating != null" class="meta">
          <span v-if="props.rating != null" class="rating">★ {{ props.rating.toFixed(1) }}</span>
          <template v-for="(m, i) in props.meta" :key="i">
            <span class="dot" />
            <span>{{ m }}</span>
          </template>
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
          <button class="btn btn-play" type="button" @click="emit('play')">▶ 播放</button>

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

          <button class="btn btn-ghost">＋ 收藏</button>
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
}

.title {
  margin: 0 0 var(--fbz-space-2);
  font-size: 40px;
  line-height: 1.1;
  font-weight: 800;
}

.tagline {
  margin: 0 0 var(--fbz-space-3);
  font-size: var(--fbz-font-size-md);
  font-style: italic;
  color: var(--fbz-color-text-muted);
}

.meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--fbz-space-3);
  margin-bottom: var(--fbz-space-3);
  font-size: var(--fbz-font-size-md);
  color: var(--fbz-color-text-soft);

  .dot {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--fbz-color-text-muted);
  }

  .rating {
    color: var(--fbz-color-brand-500);
    font-weight: 700;
  }
}

.tags {
  display: flex;
  flex-wrap: wrap;
  gap: var(--fbz-space-2);
  margin-bottom: var(--fbz-space-3);
}

.tag {
  padding: 3px 9px;
  border: 1px solid var(--fbz-color-line-bright);
  border-radius: 3px;
  font-size: var(--fbz-font-size-xs);
  font-weight: 600;
  letter-spacing: 0.4px;
  color: var(--fbz-color-text-soft);
  background: rgba(255, 255, 255, 0.04);
}

.subs {
  display: flex;
  gap: var(--fbz-space-2);
  margin-bottom: var(--fbz-space-4);
  font-size: var(--fbz-font-size-sm);
  color: var(--fbz-color-text-soft);

  .subs-label {
    color: var(--fbz-color-text-muted);
  }
}

.overview {
  max-width: 720px;
  margin: 0 0 var(--fbz-space-5);
  font-size: var(--fbz-font-size-md);
  line-height: 1.7;
  color: var(--fbz-color-text-soft);
}

.actions {
  display: flex;
  align-items: center;
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
  gap: 9px;
  cursor: pointer;
  transition:
    background var(--fbz-motion-fast),
    border-color var(--fbz-motion-fast);
}

.btn-play {
  color: #07120a;
  background: var(--fbz-color-brand-500);

  &:hover {
    background: var(--fbz-color-brand-600);
  }
}

.btn-ghost {
  color: #fff;
  background: rgba(255, 255, 255, 0.08);
  border-color: var(--fbz-color-line);

  &:hover {
    background: rgba(255, 255, 255, 0.16);
  }
}

.version-single {
  font-size: var(--fbz-font-size-sm);
  color: var(--fbz-color-text-muted);
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

  .actions {
    flex-wrap: wrap;
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
