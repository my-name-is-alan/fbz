<script setup lang="ts">
import {
  downloadRemoteSubtitle,
  searchRemoteSubtitles,
  type RemoteSubtitleInfo,
} from "@/service/modules/metadata.ts";
import type { SubtitleStreamInfo } from "@/service/modules/mediaInfo.ts";
import { useAuthStore } from "@/stores/auth.ts";
import { useUiStore } from "@/stores/ui.ts";

/**
 * 外挂字幕：RemoteSearch/Subtitles（插件 `subtitle.provider.query`，如 OpenSubtitles）。
 * 搜索对可见条目开放；下载写 sidecar 需要管服权限（后端校验，这里只做入口提示）。
 */
interface Props {
  open: boolean;
  itemId: string;
  title: string;
  mediaSourceId?: string;
  /** 当前版本已有字幕轨（只读展示）。 */
  existing?: SubtitleStreamInfo[];
}

const props = withDefaults(defineProps<Props>(), {
  existing: () => [],
});
const emit = defineEmits<{ close: []; downloaded: [] }>();

const authStore = useAuthStore();
const uiStore = useUiStore();

const LANGUAGES = [
  { label: "中文", value: "chi" },
  { label: "英语", value: "eng" },
  { label: "日语", value: "jpn" },
  { label: "韩语", value: "kor" },
];

const language = ref("chi");
const results = ref<RemoteSubtitleInfo[]>([]);
const searching = ref(false);
const searched = ref(false);
const failed = ref(false);
const downloading = ref<string | null>(null);

async function search() {
  if (!props.itemId || searching.value) return;
  searching.value = true;
  failed.value = false;
  try {
    results.value = await searchRemoteSubtitles(props.itemId, language.value, props.mediaSourceId);
  } catch {
    results.value = [];
    failed.value = true;
  } finally {
    searching.value = false;
    searched.value = true;
  }
}

async function download(sub: RemoteSubtitleInfo) {
  if (!sub.Id || downloading.value) return;
  downloading.value = sub.Id;
  try {
    await downloadRemoteSubtitle(props.itemId, sub.Id, props.mediaSourceId);
    uiStore.showToast(`已下载「${sub.Name ?? "字幕"}」为外挂字幕。`, "success");
    emit("downloaded");
  } catch (error) {
    const status = (error as { response?: { status?: number } })?.response?.status;
    uiStore.showToast(
      status === 403 ? "下载字幕需要管理员权限。" : "下载字幕失败，请稍后重试。",
      "error",
    );
  } finally {
    downloading.value = null;
  }
}

watch(
  () => props.open,
  (open) => {
    if (open && !searched.value) void search();
  },
  { immediate: true },
);

watch(
  () => props.itemId,
  () => {
    results.value = [];
    searched.value = false;
  },
);
</script>

<template>
  <BaseModal
    :open="props.open"
    title="外挂字幕"
    :description="props.title"
    wide
    @close="emit('close')"
  >
    <div class="sub-modal">
      <div class="bar">
        <div class="seg" role="group" aria-label="字幕语言">
          <button
            v-for="lang in LANGUAGES"
            :key="lang.value"
            type="button"
            :class="{ on: language === lang.value }"
            @click="
              language = lang.value;
              search();
            "
          >
            {{ lang.label }}
          </button>
        </div>
        <button
          class="du-btn du-btn-sm du-btn-ghost"
          type="button"
          :disabled="searching"
          @click="search"
        >
          <BaseIcon
            :name="searching ? 'loading' : 'refresh'"
            :size="14"
            :class="{ 'c-spin': searching }"
          />重新搜索
        </button>
      </div>

      <section v-if="props.existing.length" class="block">
        <h3><BaseIcon name="subtitles" :size="14" />已有字幕轨</h3>
        <ul class="existing">
          <li v-for="(s, i) in props.existing" :key="`${s.index ?? i}`">
            <span class="mini" :class="{ ext: s.isExternal }">{{
              s.isExternal ? "外挂" : "内嵌"
            }}</span>
            <span>{{ s.language ?? "未知" }}</span>
            <span class="c-meta">{{ s.codec ?? "" }} {{ s.title ?? "" }}</span>
          </li>
        </ul>
      </section>

      <section class="block">
        <h3><BaseIcon name="search" :size="14" />远程候选</h3>
        <p v-if="searching" class="c-state">
          <BaseIcon name="loading" :size="15" class="c-spin" />正在搜索…
        </p>
        <p v-else-if="failed" class="c-state is-error">
          <BaseIcon name="error" :size="15" />搜索失败，请稍后重试。
        </p>
        <p v-else-if="searched && !results.length" class="c-state">
          <BaseIcon
            name="search-x"
            :size="15"
          />没有找到候选。确认已安装并启用字幕插件（subtitle.provider.query）。
        </p>
        <ul v-else class="results">
          <li v-for="sub in results" :key="sub.Id ?? sub.Name ?? ''" class="result">
            <div class="r-main">
              <strong :title="sub.Name ?? ''">{{ sub.Name ?? "未命名字幕" }}</strong>
              <div class="r-meta c-meta">
                <span class="mini">{{ (sub.Format ?? "?").toUpperCase() }}</span>
                <span v-if="sub.ProviderName">{{ sub.ProviderName }}</span>
                <span v-if="sub.ThreeLetterISOLanguageName">{{
                  sub.ThreeLetterISOLanguageName
                }}</span>
                <span v-if="sub.DownloadCount"
                  ><BaseIcon name="download" :size="11" />{{ sub.DownloadCount }}</span
                >
                <span v-if="sub.CommunityRating"
                  ><BaseIcon name="star" :size="11" />{{ sub.CommunityRating }}</span
                >
                <span v-if="sub.IsHashMatch" class="mini ok">哈希匹配</span>
                <span v-if="sub.IsForced" class="mini">强制</span>
              </div>
            </div>
            <button
              class="du-btn du-btn-sm"
              :class="authStore.isAdmin ? 'du-btn-primary' : 'du-btn-ghost'"
              type="button"
              :disabled="!authStore.isAdmin || downloading !== null"
              :title="authStore.isAdmin ? '下载为外挂字幕' : '下载需要管理员权限'"
              @click="download(sub)"
            >
              <BaseIcon
                :name="downloading === sub.Id ? 'loading' : authStore.isAdmin ? 'download' : 'lock'"
                :size="14"
                :class="{ 'c-spin': downloading === sub.Id }"
              />下载
            </button>
          </li>
        </ul>
      </section>
    </div>
  </BaseModal>
</template>

<style scoped lang="scss">
.sub-modal {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.seg {
  display: inline-flex;
  padding: 2px;
  gap: 2px;
  border: 1px solid var(--ui-border);
  border-radius: 9px;
  background: var(--ui-surface);

  button {
    height: 26px;
    padding: 0 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ui-text-muted);
    font-size: 12.5px;
    cursor: pointer;

    &.on {
      color: var(--ui-text);
      background: rgb(var(--ui-accent-channel) / 14%);
      box-shadow: inset 0 0 0 1px rgb(var(--ui-accent-channel) / 30%);
    }
  }
}

.block h3 {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0 0 8px;
  color: var(--ui-text);
  font-size: 13px;
  font-weight: 600;
}

.existing,
.results {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.existing li {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
}

.result {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-surface);
}

.r-main {
  flex: 1;
  min-width: 0;

  strong {
    display: block;
    overflow: hidden;
    color: var(--ui-text);
    font-size: 13px;
    font-weight: 500;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
}

.r-meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin-top: 4px;

  span {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
}

.mini {
  height: 18px;
  padding: 0 5px;
  border: 1px solid var(--ui-border-strong);
  border-radius: 4px;
  color: var(--ui-text-muted);
  font-size: 10.5px;

  &.ext {
    border-color: rgb(var(--ui-accent-channel) / 50%);
    color: var(--ui-accent-hover);
  }

  &.ok {
    border-color: rgb(52 211 153 / 45%);
    color: var(--ui-success);
  }
}

.c-state {
  padding: 16px 0;
}
</style>
