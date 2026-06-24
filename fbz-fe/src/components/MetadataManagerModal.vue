<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";
import { imageUrl } from "@/service/modules/tmdb.ts";

const uiStore = useUiStore();
const { metadataManager } = storeToRefs(uiStore);

const activeTab = ref("basic"); // basic | poster | fanart
const localPosterUpload = ref<string | null>(null);
const localFanartUpload = ref<string | null>(null);

// Form Fields
const form = ref({
  title: "",
  originalTitle: "",
  year: 2026,
  rating: 8.5,
  genres: "",
  overview: "",
  tagline: "",
  posterLanguage: "zh",
});

// Watch modal item and populate fields
watch(
  () => metadataManager.value.item,
  (item) => {
    if (item) {
      form.value = {
        title: item.title,
        originalTitle: item.title + " (Original)",
        year: item.year || 2026,
        rating: item.rating || 8.0,
        genres: item.genre || "科幻",
        overview: "这是一个经典的影视作品元数据说明。包含出色的视觉效果与感人至深的故事桥段。",
        tagline: "宇宙的浩瀚终将与你相遇",
        posterLanguage: "zh",
      };
      localPosterUpload.value = null;
      localFanartUpload.value = null;
      activeTab.value = "basic";
    }
  },
  { immediate: true },
);

// Mock Online Posters from TMDB
const mockOnlinePosters = computed(() => {
  const item = metadataManager.value.item;
  if (!item) return [];
  // Return some realistic dummy image URLs based on item poster or placeholders
  const base = item.poster || "https://images.unsplash.com/photo-1536440136628-849c177e76a1?w=500";
  return [
    { id: "p1", url: base, label: "TMDB 官方 (中文 - 优先)", selected: !localPosterUpload.value },
    {
      id: "p2",
      url: "https://images.unsplash.com/photo-1440404653325-ab127d49abc1?w=500",
      label: "TMDB 官方 (英文)",
      selected: false,
    },
    {
      id: "p3",
      url: "https://images.unsplash.com/photo-1489599849927-2ee91cede3ba?w=500",
      label: "FanArt 社区精选",
      selected: false,
    },
    {
      id: "p4",
      url: "https://images.unsplash.com/photo-1478720143023-ac1c12bccb5b?w=500",
      label: "本地磁盘扫描",
      selected: false,
    },
  ];
});

// Mock Online Fanarts
const mockOnlineFanarts = computed(() => {
  return [
    {
      id: "f1",
      url: "https://images.unsplash.com/photo-1536440136628-849c177e76a1?w=1080",
      label: "TMDB 原创背景图 1",
      selected: !localFanartUpload.value,
    },
    {
      id: "f2",
      url: "https://images.unsplash.com/photo-1509198397868-475647b2a1e5?w=1080",
      label: "电影剧照原图 2",
      selected: false,
    },
    {
      id: "f3",
      url: "https://images.unsplash.com/photo-1518709268805-4e9042af9f23?w=1080",
      label: "极简艺术背景图 3",
      selected: false,
    },
  ];
});

const selectedPosterId = ref("p1");
const selectedFanartId = ref("f1");

function selectPoster(id: string) {
  selectedPosterId.value = id;
  localPosterUpload.value = null;
}

function selectFanart(id: string) {
  selectedFanartId.value = id;
  localFanartUpload.value = null;
}

function triggerPosterUpload() {
  const url = prompt(
    "请输入您想使用的本地海报 URL 或占位路径:",
    "https://images.unsplash.com/photo-1485846234645-a62644f84728?w=500",
  );
  if (url) {
    localPosterUpload.value = url;
    selectedPosterId.value = "";
  }
}

function triggerFanartUpload() {
  const url = prompt(
    "请输入您想使用的本地背景图 URL 或占位路径:",
    "https://images.unsplash.com/photo-1517604931442-7e0c8ed2963c?w=1080",
  );
  if (url) {
    localFanartUpload.value = url;
    selectedFanartId.value = "";
  }
}

function handleSave() {
  if (!metadataManager.value.item) return;

  // Apply visual changes locally
  const item = metadataManager.value.item;
  item.title = form.value.title;
  item.year = form.value.year;
  item.rating = form.value.rating;
  item.genre = form.value.genres;
  item.meta = `${form.value.year} · ${form.value.genres}`;

  if (localPosterUpload.value) {
    item.poster = localPosterUpload.value;
  } else {
    const selectedObj = mockOnlinePosters.value.find((p) => p.id === selectedPosterId.value);
    if (selectedObj) {
      item.poster = selectedObj.url;
    }
  }

  uiStore.showToast("元数据更新成功！已将信息同步到本地 NFO 文件并刷新图片缓存。", "success");
  uiStore.closeMetadataManager();
}

useEventListener(window, "keydown", (e) => {
  if (e.key === "Escape" && metadataManager.value.open) {
    uiStore.closeMetadataManager();
  }
});
</script>

<template>
  <Transition name="fade">
    <div v-if="metadataManager.open" class="metadata-overlay" @click="uiStore.closeMetadataManager">
      <div
        class="metadata-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="metadata-title"
        @click.stop
      >
        <!-- Modal Header -->
        <header class="modal-header">
          <div class="header-title">
            <span class="icon" aria-hidden="true">✏️</span>
            <div>
              <h2 id="metadata-title">编辑元数据</h2>
              <span class="subtitle">{{ metadataManager.item?.title }}</span>
            </div>
          </div>
          <button
            class="close-btn"
            type="button"
            aria-label="关闭元数据管理器"
            @click="uiStore.closeMetadataManager"
          >
            ✕
          </button>
        </header>

        <!-- Tabs Navigation -->
        <nav class="modal-tabs" role="tablist" aria-label="元数据编辑分类">
          <button
            class="tab-btn"
            :class="{ active: activeTab === 'basic' }"
            type="button"
            role="tab"
            :aria-selected="activeTab === 'basic'"
            aria-controls="tabpanel-basic"
            id="tab-basic"
            @click="activeTab = 'basic'"
          >
            📋 基本信息
          </button>
          <button
            class="tab-btn"
            :class="{ active: activeTab === 'poster' }"
            type="button"
            role="tab"
            :aria-selected="activeTab === 'poster'"
            aria-controls="tabpanel-poster"
            id="tab-poster"
            @click="activeTab = 'poster'"
          >
            🖼️ 封面与海报
          </button>
          <button
            class="tab-btn"
            :class="{ active: activeTab === 'fanart' }"
            type="button"
            role="tab"
            :aria-selected="activeTab === 'fanart'"
            aria-controls="tabpanel-fanart"
            id="tab-fanart"
            @click="activeTab = 'fanart'"
          >
            🌌 背景与剧照
          </button>
        </nav>

        <!-- Modal Content Body -->
        <div class="modal-body">
          <!-- Tab 1: Basic Info Form -->
          <div
            v-if="activeTab === 'basic'"
            id="tabpanel-basic"
            role="tabpanel"
            aria-labelledby="tab-basic"
            class="tab-content basic-info-tab"
          >
            <div class="form-row">
              <div class="form-group flex-2">
                <label for="meta-title">影片名称 (Title)</label>
                <input id="meta-title" v-model="form.title" type="text" class="control-input" />
              </div>
              <div class="form-group flex-1">
                <label style="opacity: 0" for="meta-lock">.</label>
                <div class="checkbox-wrapper">
                  <label class="checkbox-label" for="meta-lock">
                    <input id="meta-lock" type="checkbox" checked />
                    <span class="custom-check" />
                    <span>自动锁定该条目</span>
                  </label>
                </div>
              </div>
            </div>

            <div class="form-row">
              <div class="form-group">
                <label for="meta-orig-title">原始名称 (Original Title)</label>
                <input
                  id="meta-orig-title"
                  v-model="form.originalTitle"
                  type="text"
                  class="control-input"
                />
              </div>
              <div class="form-group flex-half">
                <label for="meta-year">上映年份 (Year)</label>
                <input
                  id="meta-year"
                  v-model.number="form.year"
                  type="number"
                  class="control-input"
                />
              </div>
              <div class="form-group flex-half">
                <label for="meta-rating">TMDB 评分 (Rating)</label>
                <input
                  id="meta-rating"
                  v-model.number="form.rating"
                  type="number"
                  step="0.1"
                  class="control-input"
                />
              </div>
            </div>

            <div class="form-row">
              <div class="form-group">
                <label for="meta-genres">题材类型 (Genres)</label>
                <input
                  id="meta-genres"
                  v-model="form.genres"
                  type="text"
                  placeholder="用逗号分隔，如：科幻, 冒险, 动作"
                  class="control-input"
                />
              </div>
              <div class="form-group">
                <label for="meta-tagline">一句话宣传语 (Tagline)</label>
                <input id="meta-tagline" v-model="form.tagline" type="text" class="control-input" />
              </div>
            </div>

            <div class="form-group">
              <label for="meta-overview">剧情简介 (Overview)</label>
              <textarea
                id="meta-overview"
                v-model="form.overview"
                rows="4"
                class="control-textarea"
              />
            </div>

            <div class="form-row">
              <div class="form-group">
                <label for="meta-lang">海报语言偏好</label>
                <select id="meta-lang" v-model="form.posterLanguage" class="control-select">
                  <option value="zh">优先中文 (CN)</option>
                  <option value="en">优先英文 (EN)</option>
                  <option value="original">使用原产国语言</option>
                </select>
              </div>
              <div class="form-group">
                <label>元数据搜刮源</label>
                <div class="source-pills">
                  <span class="source-pill active">TMDB</span>
                  <span class="source-pill active">IMDb</span>
                  <span class="source-pill">Local NFO</span>
                </div>
              </div>
            </div>
          </div>

          <!-- Tab 2: Poster Grid Selector -->
          <div
            v-if="activeTab === 'poster'"
            id="tabpanel-poster"
            role="tabpanel"
            aria-labelledby="tab-poster"
            class="tab-content assets-tab"
          >
            <div class="asset-toolbar">
              <span class="hint">为您从网络上搜刮到以下封面图片，请选择一张作为海报：</span>
              <button class="upload-btn" type="button" @click="triggerPosterUpload">
                📤 上传本地封面
              </button>
            </div>

            <div class="asset-grid poster-grid">
              <!-- Uploaded Poster Card -->
              <div v-if="localPosterUpload" class="asset-card active" title="已上传的本地封面">
                <img :src="localPosterUpload" alt="Local upload" />
                <span class="badge">本地上传</span>
              </div>

              <!-- Online Catalog Cards -->
              <div
                v-for="p in mockOnlinePosters"
                :key="p.id"
                class="asset-card"
                :class="{ active: selectedPosterId === p.id }"
                tabindex="0"
                role="button"
                :aria-pressed="selectedPosterId === p.id"
                :aria-label="`选择海报: ${p.label}`"
                @click="selectPoster(p.id)"
                @keydown.enter="selectPoster(p.id)"
                @keydown.space.prevent="selectPoster(p.id)"
              >
                <img :src="p.url" alt="Poster option" />
                <div class="asset-overlay">
                  <span class="check-icon">✓</span>
                </div>
                <span class="label">{{ p.label }}</span>
              </div>
            </div>
          </div>

          <!-- Tab 3: Fanart Grid Selector -->
          <div
            v-if="activeTab === 'fanart'"
            id="tabpanel-fanart"
            role="tabpanel"
            aria-labelledby="tab-fanart"
            class="tab-content assets-tab"
          >
            <div class="asset-toolbar">
              <span class="hint">为您搜刮到以下高画质背景与剧照横幅：</span>
              <button class="upload-btn" type="button" @click="triggerFanartUpload">
                📤 上传本地背景
              </button>
            </div>

            <div class="asset-grid fanart-grid">
              <!-- Uploaded Fanart Card -->
              <div
                v-if="localFanartUpload"
                class="asset-card active wide-card"
                title="已上传的本地背景"
              >
                <img :src="localFanartUpload" alt="Local fanart upload" />
                <span class="badge">本地上传</span>
              </div>

              <!-- Online Fanart Cards -->
              <div
                v-for="f in mockOnlineFanarts"
                :key="f.id"
                class="asset-card wide-card"
                :class="{ active: selectedFanartId === f.id }"
                tabindex="0"
                role="button"
                :aria-pressed="selectedFanartId === f.id"
                :aria-label="`选择背景图: ${f.label}`"
                @click="selectFanart(f.id)"
                @keydown.enter="selectFanart(f.id)"
                @keydown.space.prevent="selectFanart(f.id)"
              >
                <img :src="f.url" alt="Fanart option" />
                <div class="asset-overlay">
                  <span class="check-icon">✓</span>
                </div>
                <span class="label">{{ f.label }}</span>
              </div>
            </div>
          </div>
        </div>

        <!-- Modal Footer Buttons -->
        <footer class="modal-footer">
          <button class="footer-btn secondary" type="button" @click="uiStore.closeMetadataManager">
            取消
          </button>
          <button class="footer-btn primary" type="button" @click="handleSave">应用并保存</button>
        </footer>
      </div>
    </div>
  </Transition>
</template>

<style scoped lang="scss">
.metadata-overlay {
  position: fixed;
  inset: 0;
  z-index: 150;
  background: rgba(0, 0, 0, 0.7);
  backdrop-filter: blur(8px);
  display: grid;
  place-content: center;
}

.metadata-modal {
  width: 780px;
  max-width: 95vw;
  height: 560px;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
  border-radius: var(--fbz-radius-card);
  box-shadow: var(--fbz-shadow-panel);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  color: var(--fbz-color-text);
  font-family: var(--fbz-font-sans);
}

.modal-header {
  padding: var(--fbz-space-4) var(--fbz-space-5);
  border-bottom: 1px solid var(--fbz-color-line-soft);
  display: flex;
  justify-content: space-between;
  align-items: center;

  .header-title {
    display: flex;
    align-items: center;
    gap: var(--fbz-space-3);

    .icon {
      font-size: 24px;
    }

    h2 {
      margin: 0;
      font-size: var(--fbz-font-size-md);
      font-weight: 800;
    }

    .subtitle {
      font-size: var(--fbz-font-size-xs);
      color: var(--fbz-color-text-muted);
    }
  }

  .close-btn {
    background: none;
    border: 0;
    color: var(--fbz-color-text-muted);
    font-size: 16px;
    cursor: pointer;
    padding: 4px;

    &:hover {
      color: var(--fbz-color-text);
    }
  }
}

.modal-tabs {
  background: var(--fbz-color-bg-strong);
  border-bottom: 1px solid var(--fbz-color-line-soft);
  padding: 0 var(--fbz-space-4);
  display: flex;
  gap: var(--fbz-space-2);
}

.tab-btn {
  height: 44px;
  background: transparent;
  border: 0;
  border-bottom: 2px solid transparent;
  color: var(--fbz-color-text-soft);
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  padding: 0 var(--fbz-space-3);
  cursor: pointer;
  transition: all var(--fbz-motion-fast);

  &:hover {
    color: var(--fbz-color-text);
  }

  &.active {
    color: var(--fbz-color-brand-500);
    border-bottom-color: var(--fbz-color-brand-500);
  }
}

.modal-body {
  flex: 1;
  overflow-y: auto;
  padding: var(--fbz-space-5);
  background: var(--fbz-color-bg);
}

.tab-content {
  height: 100%;
}

.basic-info-tab {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.form-row {
  display: flex;
  gap: var(--fbz-space-4);

  .flex-2 {
    flex: 2;
  }
  .flex-1 {
    flex: 1;
  }
  .flex-half {
    flex: 0.5;
  }
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);
  flex: 1;

  label {
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
    color: var(--fbz-color-text-soft);
  }

  .control-input {
    height: 38px;
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    padding: 0 var(--fbz-space-3);
    color: var(--fbz-color-text);
    font-size: var(--fbz-font-size-sm);

    &:focus {
      outline: none;
      border-color: var(--fbz-color-brand-500);
    }
  }

  .control-textarea {
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    padding: var(--fbz-space-3);
    color: var(--fbz-color-text);
    font-size: var(--fbz-font-size-sm);
    resize: none;
    line-height: 1.6;

    &:focus {
      outline: none;
      border-color: var(--fbz-color-brand-500);
    }
  }

  .control-select {
    height: 38px;
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    padding: 0 var(--fbz-space-3);
    color: var(--fbz-color-text);
    font-size: var(--fbz-font-size-sm);
    cursor: pointer;

    &:focus {
      outline: none;
    }
  }
}

.checkbox-wrapper {
  height: 38px;
  display: flex;
  align-items: center;
}

.checkbox-label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  cursor: pointer;
  user-select: none;

  input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
    pointer-events: none;
  }

  .custom-check {
    width: 16px;
    height: 16px;
    border: 1px solid var(--fbz-color-line);
    border-radius: 4px;
    background: var(--fbz-color-panel-strong);
    display: grid;
    place-content: center;

    &::after {
      content: "✓";
      color: #07120a;
      font-size: 10px;
      font-weight: 900;
      opacity: 0;
    }
  }

  input:checked + .custom-check {
    border-color: var(--fbz-color-brand-500);
    background: var(--fbz-color-brand-500);

    &::after {
      opacity: 1;
    }
  }

  input:focus-visible + .custom-check {
    border-color: var(--fbz-color-brand-500);
    box-shadow: 0 0 0 3px rgba(30, 215, 96, 0.4);
  }
}

.source-pills {
  display: flex;
  gap: 8px;
  align-items: center;
  height: 38px;

  .source-pill {
    padding: 4px 10px;
    border-radius: var(--fbz-radius-round);
    background: var(--fbz-color-panel-strong);
    border: 1px solid var(--fbz-color-line);
    font-size: 11px;
    color: var(--fbz-color-text-muted);

    &.active {
      border-color: var(--fbz-color-brand-500);
      color: var(--fbz-color-brand-500);
      background: color-mix(in srgb, var(--fbz-color-brand-500) 8%, var(--fbz-color-panel-strong));
      font-weight: 600;
    }
  }
}

.assets-tab {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
  overflow: hidden;
}

.asset-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;

  .hint {
    font-size: var(--fbz-font-size-sm);
    color: var(--fbz-color-text-soft);
  }

  .upload-btn {
    height: 32px;
    padding: 0 12px;
    background: var(--fbz-color-panel-strong);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    color: var(--fbz-color-text-soft);
    font-size: var(--fbz-font-size-sm);
    font-weight: 600;
    cursor: pointer;

    &:hover {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
    }
  }
}

.asset-grid {
  flex: 1;
  overflow-y: auto;
  display: grid;
  gap: var(--fbz-space-4);
  padding: 4px;
}

.poster-grid {
  grid-template-columns: repeat(4, 1fr);
}

.fanart-grid {
  grid-template-columns: repeat(2, 1fr);
}

.asset-card {
  position: relative;
  aspect-ratio: 2 / 3;
  border: 2px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel);
  border-radius: var(--fbz-radius-card);
  overflow: hidden;
  cursor: pointer;
  transition: all var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-line-bright);
    transform: translateY(-2px);
  }

  &.active {
    border-color: var(--fbz-color-brand-500);
    box-shadow: 0 4px 12px color-mix(in srgb, var(--fbz-color-brand-500) 15%, transparent);

    .asset-overlay {
      opacity: 1;
    }
  }

  &.wide-card {
    aspect-ratio: 16 / 9;
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .asset-overlay {
    position: absolute;
    inset: 0;
    background: rgba(30, 215, 96, 0.15);
    display: grid;
    place-content: center;
    opacity: 0;
    transition: opacity var(--fbz-motion-fast);

    .check-icon {
      width: 28px;
      height: 28px;
      background: var(--fbz-color-brand-500);
      color: #07120a;
      border-radius: 50%;
      display: grid;
      place-content: center;
      font-size: 16px;
      font-weight: 900;
    }
  }

  .label {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    background: rgba(0, 0, 0, 0.7);
    padding: 6px;
    font-size: 10px;
    color: var(--fbz-color-text-soft);
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .badge {
    position: absolute;
    top: 6px;
    left: 6px;
    padding: 2px 6px;
    border-radius: 3px;
    background: var(--fbz-color-brand-500);
    color: #07120a;
    font-size: 9px;
    font-weight: 700;
  }
}

.modal-footer {
  padding: var(--fbz-space-4) var(--fbz-space-5);
  border-top: 1px solid var(--fbz-color-line-soft);
  display: flex;
  justify-content: flex-end;
  gap: var(--fbz-space-2);
  background: var(--fbz-color-bg-strong);

  .footer-btn {
    height: 38px;
    padding: 0 var(--fbz-space-5);
    border-radius: var(--fbz-radius-control);
    font-size: var(--fbz-font-size-sm);
    font-weight: 700;
    cursor: pointer;
    transition: all var(--fbz-motion-fast);

    &.secondary {
      background: var(--fbz-color-panel-strong);
      border: 1px solid var(--fbz-color-line);
      color: var(--fbz-color-text-soft);

      &:hover {
        background: var(--fbz-color-panel-elevated);
        color: var(--fbz-color-text);
      }
    }

    &.primary {
      background: var(--fbz-color-brand-500);
      border: 0;
      color: #07120a;

      &:hover {
        background: var(--fbz-color-brand-600);
      }
    }
  }
}

// Fade transitions
.fade-enter-active,
.fade-leave-active {
  transition: opacity var(--fbz-motion-base) ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
