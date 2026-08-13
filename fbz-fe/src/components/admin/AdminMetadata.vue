<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";
import {
  getMetadataSettings,
  setMetadataProviderKey,
  testMetadataProvider,
  updateMetadataProviderSettings,
  updateMetadataSettings,
} from "@/service/modules/admin.ts";

const uiStore = useUiStore();

// Form states
const selectedLanguage = ref("zh-CN");
const tmdbToken = ref("");
const tmdbHasKey = ref(false);
const showToken = ref(false);
const loading = ref(false);
const loadError = ref<string | null>(null);

/**
 * 刮削器列表与后端 provider registry 一一对应（tmdb/tvdb/fanart/imdb/spotify）。
 * TMDB 是内置兜底刮削源；第三方刮削器以插件形式接入（订阅
 * `metadata.provider.query`），在插件设置页管理，不在此列表。
 */
const scrapers = ref([
  {
    id: "tmdb",
    name: "The Movie Database (TMDB)",
    desc: "内置兜底搜刮源：电影与剧集元数据、海报墙的核心来源。",
    enabled: true,
  },
  {
    id: "tvdb",
    name: "TheTVDB",
    desc: "剧集元数据补充来源，含分集与播出信息。",
    enabled: true,
  },
  {
    id: "fanart",
    name: "Fanart.tv",
    desc: "高质量海报、Logo 与背景图增强。",
    enabled: true,
  },
  {
    id: "imdb",
    name: "Internet Movie Database (IMDb)",
    desc: "外部 ID 规范化与评分补充（富化，不参与基础匹配）。",
    enabled: false,
  },
  {
    id: "spotify",
    name: "Spotify",
    desc: "音乐专辑/艺人元数据来源。",
    enabled: false,
  },
]);

/**
 * 接入真实后端：拉取元数据设置并叠加到表单。开关状态优先按全局
 * providerOrder 推断（在序即启用），provider 行存在时以其 enabled 为准；
 * 后端永不回显明文 key，已配置时展示「已配置」状态。
 */
async function loadSettings() {
  loading.value = true;
  loadError.value = null;
  try {
    const settings = await getMetadataSettings();
    const byId = new Map(settings.providers.map((p) => [p.providerId, p]));
    const order = settings.global.providerOrder;
    for (const scraper of scrapers.value) {
      const provider = byId.get(scraper.id);
      if (provider) {
        scraper.enabled = provider.enabled;
      } else if (order.length) {
        scraper.enabled = order.includes(scraper.id);
      }
    }
    const lang = settings.global.defaultLanguage;
    if (lang) {
      selectedLanguage.value = lang;
    }
    tmdbHasKey.value = byId.get("tmdb")?.hasKey ?? false;
    tmdbToken.value = "";
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

const saving = ref(false);
const testing = ref(false);

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

/** 连通性探测：用当前已保存的 key/代理对 TMDB 做一次受控探测。 */
async function handleTestTmdb() {
  testing.value = true;
  try {
    const result = await testMetadataProvider("tmdb");
    uiStore.showToast(
      result.ok ? "TMDB 连通性正常。" : `TMDB 探测失败：${result.message}`,
      result.ok ? "success" : "warning",
    );
  } catch {
    uiStore.showToast("TMDB 探测请求失败，请检查后端。", "error");
  } finally {
    testing.value = false;
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
                :placeholder="tmdbHasKey ? '已配置（留空保持不变）' : '输入 TMDB 官方 API 令牌'"
              />
              <button
                class="action-btn"
                type="button"
                :aria-label="showToken ? '隐藏令牌' : '显示令牌'"
                @click="showToken = !showToken"
              >
                {{ showToken ? "隐藏" : "显示" }}
              </button>
              <button class="action-btn" type="button" :disabled="testing" @click="handleTestTmdb">
                {{ testing ? "探测中..." : "测试连通" }}
              </button>
            </div>
            <span class="field-hint"
              >保存后后端会加密存储令牌，永不回显明文；刮削时下载的图片会缓存到本地。</span
            >
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
