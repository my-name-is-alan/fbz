<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";
import {
  getMetadataSettings,
  setMetadataProviderKey,
  updateMetadataProviderSettings,
  updateMetadataSettings,
} from "@/service/modules/admin.ts";

const uiStore = useUiStore();

// Form states
const selectedLanguage = ref("zh-CN");
const tmdbToken = ref("");
const showToken = ref(false);
const loading = ref(false);
const loadError = ref<string | null>(null);

const scrapers = ref([
  {
    id: "tmdb",
    name: "The Movie Database (TMDB)",
    desc: "电影与剧集元数据、海报墙的核心搜刮来源。",
    enabled: true,
  },
  {
    id: "imdb",
    name: "Internet Movie Database (IMDb)",
    desc: "补充影片评分、专业演职员表和分级数据。",
    enabled: true,
  },
  {
    id: "nfo",
    name: "本地 NFO/本地海报优先",
    desc: "优先读取本地视频同目录下的 NFO 元数据及剧照。",
    enabled: false,
  },
]);

const scanFrequency = ref("monitor");

/**
 * 接入真实后端：拉取元数据设置并叠加到表单。命中后端时按 provider id 同步开关；
 * 后端永不回显明文 key，已配置时只给空输入框提示。
 */
async function loadSettings() {
  loading.value = true;
  loadError.value = null;
  try {
    const settings = await getMetadataSettings();
    const byId = new Map(settings.providers.map((p) => [p.providerId, p]));
    for (const scraper of scrapers.value) {
      const provider = byId.get(scraper.id);
      if (provider) {
        scraper.enabled = provider.enabled;
      }
    }
    const lang = settings.global.defaultLanguage;
    if (lang) {
      selectedLanguage.value = lang;
    }
    const tmdb = byId.get("tmdb");
    if (tmdb?.hasKey) {
      // 后端不回显明文 key，仅以占位提示「已配置」。
      tmdbToken.value = "";
    }
  } catch {
    loadError.value = "元数据设置加载失败，请检查服务器连接或管理员权限。";
  } finally {
    loading.value = false;
  }
}

onMounted(loadSettings);

const languageOptions = [
  { label: "简体中文 (zh-CN)", value: "zh-CN" },
  { label: "英语 (en-US)", value: "en-US" },
  { label: "不指定语言", value: "" },
];

const frequencyOptions = [
  { label: "实时目录监控 (建议)", value: "monitor" },
  { label: "每小时全盘自动扫描", value: "hourly" },
  { label: "每日定时全盘扫描", value: "daily" },
  { label: "仅手动触发扫描", value: "manual" },
];

const saving = ref(false);

async function handleSave() {
  saving.value = true;
  try {
    await updateMetadataSettings({
      providerOrder: scrapers.value
        .filter((scraper) => scraper.enabled)
        .map((scraper) => scraper.id),
      defaultLanguage: selectedLanguage.value || null,
      defaultCountry: null,
      imageLanguage: selectedLanguage.value || null,
      imagePreferOriginal: false,
      imageFallbackLanguages: selectedLanguage.value ? [selectedLanguage.value] : [],
    });

    await Promise.all(
      scrapers.value.map((scraper) =>
        updateMetadataProviderSettings(scraper.id, {
          enabled: scraper.enabled,
          language: selectedLanguage.value || null,
          country: null,
          imageLanguage: selectedLanguage.value || null,
          imagePreferOriginal: false,
        }),
      ),
    );

    const token = tmdbToken.value.trim();
    if (token) {
      await setMetadataProviderKey("tmdb", token);
      tmdbToken.value = "";
    }

    await loadSettings();
    uiStore.showToast("元数据设置已保存到 Rust 后端。", "success");
  } catch {
    uiStore.showToast("保存元数据设置失败，请检查后端响应。", "error");
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div class="admin-metadata-view">
    <div class="settings-stack">
      <!-- Section 1: Scraper Priority -->
      <section class="settings-card">
        <div class="card-header">
          <span class="indicator" />
          <h3>搜刮引擎列表及状态</h3>
        </div>
        <div class="card-body">
          <p v-if="loadError" class="settings-error">{{ loadError }}</p>
          <p v-else-if="loading" class="settings-hint">正在读取后端元数据设置...</p>
          <p class="settings-hint">选择启用的搜刮引擎。系统将按照优先级从上到下查找元数据。</p>

          <div class="scrapers-list">
            <div v-for="s in scrapers" :key="s.id" class="scraper-row">
              <div class="scraper-info">
                <span class="scraper-name">{{ s.name }}</span>
                <span class="scraper-desc">{{ s.desc }}</span>
              </div>
              <label class="glow-switch" :aria-label="`启用 ${s.name}`">
                <input type="checkbox" v-model="s.enabled" />
                <span class="switch-slide-thumb" />
              </label>
            </div>
          </div>
        </div>
      </section>

      <!-- Section 2: Scraper Parameters -->
      <section class="settings-card">
        <div class="card-header">
          <span class="indicator" />
          <h3>全球化搜刮配置</h3>
        </div>
        <div class="card-body">
          <!-- Preferred Language -->
          <div class="form-group">
            <label for="meta-pref-lang">搜刮元数据语言偏好</label>
            <BaseSelect
              id="meta-pref-lang"
              v-model="selectedLanguage"
              :options="languageOptions"
              ariaLabel="选择首选搜刮语言"
            />
          </div>

          <!-- TMDB Token -->
          <div class="form-group">
            <label for="meta-tmdb-token">TMDB API 令牌 / Token</label>
            <div class="input-with-action">
              <input
                id="meta-tmdb-token"
                v-model="tmdbToken"
                :type="showToken ? 'text' : 'password'"
                class="control-input"
                placeholder="输入 TMDB 官方 API 令牌"
              />
              <button
                class="action-btn"
                type="button"
                :aria-label="showToken ? '隐藏令牌' : '显示令牌'"
                @click="showToken = !showToken"
              >
                {{ showToken ? "隐藏" : "显示" }}
              </button>
            </div>
            <span class="field-hint"
              >保存后后端会加密存储令牌；扫描/刷新元数据时会下载图片到本地缓存。</span
            >
          </div>
        </div>
      </section>

      <!-- Section 3: Scanning triggers -->
      <section class="settings-card">
        <div class="card-header">
          <span class="indicator" />
          <h3>物理路径扫描策略</h3>
        </div>
        <div class="card-body">
          <div class="form-group">
            <label for="meta-scan-frequency">自动更新触发机制</label>
            <BaseSelect
              id="meta-scan-frequency"
              v-model="scanFrequency"
              :options="frequencyOptions"
              ariaLabel="选择自动更新扫描频率"
            />
          </div>
        </div>
      </section>

      <!-- Actions Footer -->
      <footer class="actions-footer">
        <button class="btn-primary" type="button" :disabled="saving" @click="handleSave">
          <span class="spinner" v-if="saving" />
          <span>{{ saving ? "正在保存..." : "保存元数据设置" }}</span>
        </button>
      </footer>
    </div>
  </div>
</template>

<style scoped lang="scss">
.admin-metadata-view {
  max-width: 800px;
}

.settings-stack {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.settings-error {
  margin: 0;
  color: var(--fbz-color-danger-500);
  font-size: var(--fbz-font-size-sm);
  line-height: 1.5;
}

.scrapers-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.scraper-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--fbz-space-5);
  padding: 12px var(--fbz-space-4);
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;

  .scraper-info {
    display: flex;
    flex-direction: column;
    gap: 4px;

    .scraper-name {
      font-size: 13px;
      font-weight: 700;
      color: var(--fbz-color-text);
    }

    .scraper-desc {
      font-size: 11px;
      color: var(--fbz-color-text-muted);
      line-height: 1.4;
    }
  }
}

.glow-switch {
  position: relative;
  display: inline-block;
  width: 44px;
  height: 22px;
  flex-shrink: 0;

  input {
    opacity: 0;
    width: 0;
    height: 0;
  }

  .switch-slide-thumb {
    position: absolute;
    cursor: pointer;
    inset: 0;
    background-color: var(--fbz-color-line-bright);
    border-radius: 22px;
    transition: background-color var(--fbz-motion-fast);

    &::before {
      position: absolute;
      content: "";
      height: 16px;
      width: 16px;
      left: 3px;
      bottom: 3px;
      background-color: white;
      border-radius: 50%;
      transition: transform var(--fbz-motion-fast);
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
    }
  }

  input:checked + .switch-slide-thumb {
    background-color: var(--fbz-color-brand-500);

    &::before {
      transform: translateX(22px);
    }
  }

  input:focus-visible + .switch-slide-thumb {
    box-shadow: var(--fbz-shadow-focus);
  }
}

.input-with-action {
  display: flex;
  gap: var(--fbz-space-2);

  input {
    flex: 1;
  }

  .action-btn {
    height: 38px;
    padding: 0 var(--fbz-space-4);
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    color: var(--fbz-color-text-soft);
    font-weight: 700;
    font-size: var(--fbz-font-size-sm);
    cursor: pointer;
    transition: all var(--fbz-motion-fast);

    &:hover {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
    }
  }
}

.field-hint {
  font-size: 11px;
  color: var(--fbz-color-text-muted);
  line-height: 1.4;
}

.actions-footer {
  display: flex;
  justify-content: flex-start;
  padding-top: var(--fbz-space-2);
}

.btn-primary {
  height: 38px;
  padding: 0 var(--fbz-space-6);
  background: var(--fbz-color-brand-500);
  border: 0;
  color: #07120a;
  font-weight: 700;
  font-size: var(--fbz-font-size-sm);
  border-radius: var(--fbz-radius-control);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  transition: all var(--fbz-motion-fast);

  &:hover:not(:disabled) {
    background: var(--fbz-color-brand-600);
  }

  &:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
}

.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid #07120a;
  border-top-color: transparent;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
