<script setup lang="ts">
import type { MediaVersion } from "@/types/media.ts";
import { formatClock } from "@/service/modules/mediaInfo.ts";

/**
 * 详情页头部（HDHive / Emby 气质）：剧照铺底 + 渐变遮罩，海报在左，元信息在右。
 * - 复合规格徽章随所选版本切换（分辨率 · 动态范围 · 编码 · 音轨 · 容器）
 * - 多版本下拉（v-model:version-id，与「媒体信息」面板同步）
 * - 播放 / 续播 / 从头、收藏、已看、更多（右键菜单同款动作集）
 */
interface Props {
  title: string;
  originalTitle?: string;
  poster?: string;
  backdrop?: string;
  /** 标题下方的信息片段，如 ["2024", "2h 6m"] */
  meta?: string[];
  genres?: string[];
  officialRating?: string;
  overview?: string;
  rating?: number | null;
  /** 类型标识（电影 / 单集 / 视频 …）+ 图标 */
  typeLabel?: string;
  typeIcon?: string;
  /** 海报比例：单集 / 家庭视频可用宽幅缩略图 */
  posterRatio?: "poster" | "wide";
  /** 是否显示播放按钮（人物页不需要） */
  showActions?: boolean;
  /** 可选播放版本；多版本时显示下拉 */
  versions?: MediaVersion[];
  isFavorite?: boolean;
  /** 续播位置（秒）；有值时主按钮变「继续播放」并附「从头播放」 */
  resumePositionSeconds?: number;
  played?: boolean;
  /** 显示「已看」切换按钮（需要条目 id 的页面才开） */
  showPlayedToggle?: boolean;
  /** 显示「更多」按钮（打开条目右键菜单） */
  showMore?: boolean;
  busy?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  meta: () => [],
  genres: () => [],
  posterRatio: "poster",
  showActions: true,
  versions: () => [],
  isFavorite: false,
  played: false,
  showPlayedToggle: false,
  showMore: false,
  busy: false,
});

const emit = defineEmits<{
  /** 播放（从头）。载荷 = 所选版本 id（无版本信息时 undefined）。 */
  play: [versionId?: string];
  /** 续播（从 resumePositionSeconds 继续）。 */
  resume: [versionId?: string];
  toggleFavorite: [];
  togglePlayed: [];
  more: [event: MouseEvent];
}>();

const versionId = defineModel<string>("versionId", { default: "" });

watch(
  () => props.versions,
  (list) => {
    if (!list.some((v) => v.id === versionId.value)) versionId.value = list[0]?.id ?? "";
  },
  { immediate: true },
);

const activeVersion = computed(
  () => props.versions.find((v) => v.id === versionId.value) ?? props.versions[0],
);
const versionOptions = computed(() => props.versions.map((v) => ({ label: v.label, value: v.id })));

const resumeLabel = computed(() => formatClock(props.resumePositionSeconds));
</script>

<template>
  <header class="detail-hero">
    <div class="backdrop" aria-hidden="true">
      <img v-if="props.backdrop" :src="props.backdrop" alt="" />
      <div class="backdrop-scrim" />
    </div>

    <div class="hero-head">
      <slot name="header" />
    </div>

    <div class="hero-body">
      <div class="poster" :class="`is-${props.posterRatio}`">
        <MediaPoster
          :src="props.poster"
          :title="props.title"
          :ratio="props.posterRatio"
          :icon="props.typeIcon"
        />
        <span v-if="props.played" class="poster-flag" title="已看完">
          <BaseIcon name="check" :size="14" :stroke-width="3" />
        </span>
      </div>

      <div class="info">
        <span v-if="props.typeLabel" class="type-chip">
          <BaseIcon v-if="props.typeIcon" :name="props.typeIcon" :size="12" />{{ props.typeLabel }}
        </span>
        <h1 class="title">{{ props.title }}</h1>
        <p v-if="props.originalTitle && props.originalTitle !== props.title" class="original">
          {{ props.originalTitle }}
        </p>

        <div class="meta c-meta">
          <span v-if="props.rating != null && props.rating > 0" class="rating">
            <BaseIcon name="star" :size="13" fill :stroke-width="2" />{{ props.rating.toFixed(1) }}
          </span>
          <template v-for="(m, i) in props.meta" :key="`${m}-${i}`">
            <span v-if="i > 0 || (props.rating != null && props.rating > 0)" class="c-dot" />
            <span>{{ m }}</span>
          </template>
          <span v-if="props.genres.length" class="c-dot" />
          <span v-if="props.genres.length">{{ props.genres.slice(0, 4).join(" / ") }}</span>
          <span v-if="props.officialRating" class="cert">{{ props.officialRating }}</span>
        </div>

        <div v-if="activeVersion?.tags.length" class="specs" aria-label="规格">
          <span
            v-for="(t, i) in activeVersion.tags"
            :key="t"
            class="spec"
            :class="{ strong: i === 0, accent: /HDR|Dolby|HLG/.test(t) }"
            >{{ t }}</span
          >
          <span
            v-if="props.versions.length > 1"
            class="spec ver"
            :title="`${props.versions.length} 个版本`"
          >
            <BaseIcon name="versions" :size="11" :stroke-width="2.2" />{{ props.versions.length }}
            版本
          </span>
          <span
            v-if="activeVersion.subtitles.length"
            class="spec subs"
            :title="activeVersion.subtitles.join(' / ')"
          >
            <BaseIcon name="subtitles" :size="11" :stroke-width="2.2" />{{
              activeVersion.subtitles.length
            }}
            字幕
          </span>
        </div>

        <div v-if="props.showActions" class="actions">
          <template v-if="props.resumePositionSeconds">
            <button
              class="du-btn du-btn-primary act-main"
              type="button"
              :disabled="props.busy"
              @click="emit('resume', activeVersion?.id)"
            >
              <BaseIcon name="play" :size="16" fill />继续播放<span class="sub">{{
                resumeLabel
              }}</span>
            </button>
            <button
              class="du-btn act"
              type="button"
              :disabled="props.busy"
              @click="emit('play', activeVersion?.id)"
            >
              <BaseIcon name="rewind" :size="15" />从头播放
            </button>
          </template>
          <button
            v-else
            class="du-btn du-btn-primary act-main"
            type="button"
            :disabled="props.busy"
            @click="emit('play', activeVersion?.id)"
          >
            <BaseIcon
              :name="props.busy ? 'loading' : 'play'"
              :size="16"
              :fill="!props.busy"
              :class="{ 'c-spin': props.busy }"
            />播放
          </button>

          <label v-if="props.versions.length > 1" class="version-pick">
            <BaseIcon name="versions" :size="15" />
            <BaseSelect
              v-model="versionId"
              :options="versionOptions"
              size="sm"
              aria-label="选择版本"
            />
          </label>

          <button
            class="du-btn act icon"
            :class="{ 'is-on fav': props.isFavorite }"
            type="button"
            :aria-pressed="props.isFavorite"
            :title="props.isFavorite ? '取消收藏' : '收藏'"
            @click="emit('toggleFavorite')"
          >
            <BaseIcon name="heart" :size="16" :fill="props.isFavorite" />
            <span class="lbl">{{ props.isFavorite ? "已收藏" : "收藏" }}</span>
          </button>
          <button
            v-if="props.showPlayedToggle"
            class="du-btn act icon"
            :class="{ 'is-on': props.played }"
            type="button"
            :aria-pressed="props.played"
            :title="props.played ? '标记未看' : '标记已看'"
            @click="emit('togglePlayed')"
          >
            <BaseIcon name="check" :size="16" :stroke-width="props.played ? 2.6 : 1.7" />
            <span class="lbl">{{ props.played ? "已看" : "标记已看" }}</span>
          </button>
          <slot name="actions" />
          <button
            v-if="props.showMore"
            class="du-btn act square"
            type="button"
            aria-label="更多操作"
            title="更多操作"
            @click.stop="emit('more', $event)"
          >
            <BaseIcon name="more" :size="18" />
          </button>
        </div>

        <p v-if="props.overview" class="overview">{{ props.overview }}</p>

        <div v-if="$slots.facts" class="facts">
          <slot name="facts" />
        </div>
        <slot name="extra" />
      </div>
    </div>
  </header>
</template>

<style scoped lang="scss">
.detail-hero {
  position: relative;
  isolation: isolate;
  // 宿主页面（.c-page.detail-page）不留顶部内边距，剧照贴着顶栏铺开。
  margin: 0 calc(var(--ui-shell-gutter) * -1);
  padding: 16px var(--ui-shell-gutter) 8px;
}

.backdrop {
  position: absolute;
  inset: 0 0 auto;
  z-index: -1;
  height: min(620px, 92%);
  overflow: hidden;

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center 20%;
    opacity: 0.55;
  }
}

.backdrop-scrim {
  position: absolute;
  inset: 0;
  background:
    linear-gradient(
      180deg,
      rgb(var(--ui-scrim) / 30%) 0%,
      rgb(var(--ui-scrim) / 78%) 60%,
      var(--ui-background) 100%
    ),
    linear-gradient(90deg, rgb(var(--ui-scrim) / 80%) 0%, rgb(var(--ui-scrim) / 0%) 70%);
}

.hero-head {
  margin-bottom: 28px;
}

.hero-body {
  display: flex;
  gap: 36px;
  align-items: flex-end;
}

.poster {
  position: relative;
  flex: 0 0 auto;
  width: 260px;
  overflow: hidden;
  border: 1px solid var(--ui-glass-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-surface-raised);
  box-shadow: var(--ui-shadow-float);

  &.is-wide {
    width: 400px;
  }
}

.poster-flag {
  position: absolute;
  top: 8px;
  left: 8px;
  width: 26px;
  height: 26px;
  display: grid;
  place-items: center;
  border-radius: 50%;
  background: var(--ui-accent);
  color: var(--ui-accent-contrast);
  box-shadow: 0 2px 10px rgb(0 0 0 / 45%);
}

.info {
  flex: 1;
  min-width: 0;
  padding-bottom: 4px;
}

.type-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  margin-bottom: 10px;
  padding: 0 8px;
  border: 1px solid var(--ui-glass-border);
  border-radius: 5px;
  background: var(--ui-glass);
  color: var(--ui-text-muted);
  font-size: 11.5px;
  font-weight: 600;
}

.title {
  margin: 0;
  color: var(--ui-text);
  font-size: clamp(28px, 3.6vw, 44px);
  font-weight: 700;
  line-height: 1.12;
  letter-spacing: -0.02em;
}

.original {
  margin: 6px 0 0;
  color: var(--ui-text-muted);
  font-size: 14px;
}

.meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin-top: 12px;
  font-size: 12.5px;
}

.rating {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--ui-warning);
  font-weight: 700;
}

.cert {
  height: 19px;
  padding: 0 5px;
  display: inline-flex;
  align-items: center;
  border: 1px solid var(--ui-border-strong);
  border-radius: 4px;
  color: var(--ui-text-muted);
  font-size: 10.5px;
  font-weight: 700;
}

.specs {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 12px;
}

.spec {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  height: 22px;
  padding: 0 7px;
  border: 1px solid var(--ui-border-strong);
  border-radius: 5px;
  background: rgb(0 0 0 / 22%);
  color: var(--ui-text-muted);
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.02em;

  &.strong {
    color: var(--ui-text);
  }

  &.accent {
    border-color: rgb(var(--ui-accent-channel) / 55%);
    color: var(--ui-accent-hover);
  }

  &.ver,
  &.subs {
    font-weight: 600;
  }
}

.actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin-top: 20px;
}

.act-main {
  height: 40px;
  min-height: 40px;
  padding: 0 20px;
  border-radius: var(--ui-radius-md);

  .sub {
    margin-left: 2px;
    opacity: 0.72;
    font-family: var(--ui-font-mono);
    font-size: 12px;
  }
}

.act {
  height: 40px;
  min-height: 40px;
  padding: 0 14px;
  border: 1px solid var(--ui-border-strong);
  border-radius: var(--ui-radius-md);
  background: rgb(0 0 0 / 18%);
  color: var(--ui-text);

  &:hover {
    background: var(--ui-surface-hover);
    border-color: var(--ui-border-strong);
  }

  &.square {
    width: 40px;
    padding: 0;
  }

  &.is-on {
    border-color: rgb(var(--ui-accent-channel) / 60%);
    color: var(--ui-accent-hover);
  }

  &.is-on.fav {
    border-color: rgb(251 113 133 / 55%);
    color: #fb7185;
  }
}

:slotted(.act) {
  height: 40px;
  min-height: 40px;
  padding: 0 14px;
  border: 1px solid var(--ui-border-strong);
  border-radius: var(--ui-radius-md);
  background: rgb(0 0 0 / 18%);
  color: var(--ui-text);
}

.version-pick {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 40px;
  padding: 0 4px 0 10px;
  border: 1px solid var(--ui-border-strong);
  border-radius: var(--ui-radius-md);
  background: rgb(0 0 0 / 18%);
  color: var(--ui-text-muted);

  :deep(.base-select) {
    max-width: 260px;
    border: 0;
    background: transparent;
    box-shadow: none;
  }
}

.overview {
  max-width: 880px;
  margin: 20px 0 0;
  color: var(--ui-text-muted);
  font-size: 13.5px;
  line-height: 1.75;
  white-space: pre-line;
}

.facts {
  margin-top: 18px;
}

@media (max-width: 900px) {
  .hero-body {
    flex-direction: column;
    // stretch：信息区占满容器宽度，操作按钮才能换行而不是撑出屏幕。
    align-items: stretch;
    gap: 20px;
  }

  .poster {
    align-self: flex-start;
    width: 180px;

    &.is-wide {
      width: min(100%, 420px);
    }
  }

  .act .lbl {
    display: none;
  }
}
</style>
