<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";
import { useBodyScrollLock } from "@/composables/useBodyScrollLock.ts";
import {
  applyRemoteSearch,
  remoteSearchItems,
  type RemoteSearchCandidate,
  type RemoteSearchKind,
} from "@/service/modules/metadata.ts";

const uiStore = useUiStore();
const { identifyManager } = storeToRefs(uiStore);
useBodyScrollLock(() => identifyManager.value.open);

const searchName = ref("");
const searchYear = ref("");
const searching = ref(false);
const applying = ref(false);
const results = ref<RemoteSearchCandidate[]>([]);
const searched = ref(false);

function kindForItem(): RemoteSearchKind {
  const item = identifyManager.value.item;
  return item?.detailType === "tv" || item?.libraryId === "series" ? "Series" : "Movie";
}

watch(
  () => identifyManager.value.item,
  (item) => {
    if (!item) return;
    searchName.value = item.title;
    searchYear.value = item.year != null ? String(item.year) : "";
    results.value = [];
    searched.value = false;
  },
  { immediate: true },
);

async function runSearch() {
  const item = identifyManager.value.item;
  if (!item || searching.value) return;
  const name = searchName.value.trim();
  if (!name) {
    uiStore.showToast("请输入要搜索的标题。", "warning");
    return;
  }
  searching.value = true;
  searched.value = false;
  try {
    const yearNum = Number(searchYear.value);
    results.value = await remoteSearchItems({
      kind: kindForItem(),
      itemId: item.id,
      name,
      year: Number.isFinite(yearNum) && yearNum > 0 ? yearNum : undefined,
    });
    searched.value = true;
    if (!results.value.length) {
      uiStore.showToast("未找到匹配结果，可调整标题或年份后再试。", "info");
    }
  } catch {
    results.value = [];
    uiStore.showToast("识别搜索失败，请确认账号具备管理权限。", "error");
  } finally {
    searching.value = false;
  }
}

async function applyCandidate(candidate: RemoteSearchCandidate) {
  const item = identifyManager.value.item;
  if (!item || applying.value) return;
  if (!candidate.ProviderIds || !Object.keys(candidate.ProviderIds).length) {
    uiStore.showToast("该候选缺少 ProviderIds，无法应用。", "warning");
    return;
  }
  applying.value = true;
  try {
    await applyRemoteSearch(item.id, candidate, false);
    uiStore.showToast("已应用识别结果，元数据刷新已入队。", "success");
    uiStore.closeIdentifyManager();
  } catch {
    uiStore.showToast("应用识别失败，请稍后重试。", "error");
  } finally {
    applying.value = false;
  }
}

useEventListener(window, "keydown", (e) => {
  if (e.key === "Escape" && identifyManager.value.open) {
    uiStore.closeIdentifyManager();
  }
});
</script>

<template>
  <Transition name="fade">
    <div v-if="identifyManager.open" class="identify-overlay" @click="uiStore.closeIdentifyManager">
      <div
        class="identify-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="identify-title"
        @click.stop
      >
        <header class="modal-header">
          <div>
            <h2 id="identify-title">识别</h2>
            <p class="subtitle">{{ identifyManager.item?.title }}</p>
          </div>
          <button
            class="close-btn"
            type="button"
            aria-label="关闭"
            @click="uiStore.closeIdentifyManager"
          >
            ✕
          </button>
        </header>

        <form class="search-row" @submit.prevent="runSearch">
          <input
            v-model="searchName"
            class="field"
            type="search"
            placeholder="标题"
            aria-label="识别标题"
          />
          <input
            v-model="searchYear"
            class="field year"
            type="text"
            inputmode="numeric"
            placeholder="年份"
            aria-label="识别年份"
          />
          <button class="search-btn" type="submit" :disabled="searching">
            {{ searching ? "搜索中…" : "搜索" }}
          </button>
        </form>

        <div class="results" role="list">
          <p v-if="!searched && !searching" class="hint">
            输入标题搜索外部元数据源，选中后应用并刷新。
          </p>
          <p v-else-if="searching" class="hint">正在搜索…</p>
          <p v-else-if="searched && !results.length" class="hint">没有匹配的候选。</p>
          <button
            v-for="(c, i) in results"
            :key="`${c.Name}-${c.ProductionYear}-${i}`"
            class="candidate"
            type="button"
            role="listitem"
            :disabled="applying"
            @click="applyCandidate(c)"
          >
            <img v-if="c.ImageUrl" :src="c.ImageUrl" alt="" class="thumb" />
            <div class="meta">
              <strong>{{ c.Name || "未命名" }}</strong>
              <span>
                {{ c.ProductionYear || "—" }}
                <template v-if="c.SearchProviderName"> · {{ c.SearchProviderName }}</template>
              </span>
              <small v-if="c.Overview">{{ c.Overview }}</small>
            </div>
          </button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped lang="scss">
.identify-overlay {
  position: fixed;
  inset: 0;
  z-index: 1200;
  display: grid;
  place-items: center;
  padding: var(--fbz-space-4);
  background: color-mix(in srgb, #000 55%, transparent);
}

.identify-modal {
  width: min(560px, 100%);
  max-height: min(80vh, 720px);
  display: flex;
  flex-direction: column;
  border-radius: var(--fbz-radius-panel);
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel);
  box-shadow: var(--fbz-shadow-elevated, 0 16px 48px rgb(0 0 0 / 35%));
  overflow: hidden;
}

.modal-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--fbz-space-3);
  padding: var(--fbz-space-4) var(--fbz-space-5);
  border-bottom: 1px solid var(--fbz-color-line-soft);

  h2 {
    margin: 0;
    font-size: var(--fbz-font-size-lg);
  }

  .subtitle {
    margin: 4px 0 0;
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-sm);
  }
}

.close-btn {
  width: 32px;
  height: 32px;
  border: none;
  border-radius: var(--fbz-radius-control);
  background: transparent;
  color: var(--fbz-color-text-muted);
  cursor: pointer;

  &:hover {
    background: var(--fbz-color-panel-strong);
    color: var(--fbz-color-text);
  }
}

.search-row {
  display: flex;
  gap: var(--fbz-space-2);
  padding: var(--fbz-space-4) var(--fbz-space-5);
}

.field {
  flex: 1;
  height: 36px;
  padding: 0 12px;
  border-radius: var(--fbz-radius-control);
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel-strong);
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-sm);

  &.year {
    flex: 0 0 88px;
  }
}

.search-btn {
  height: 36px;
  padding: 0 14px;
  border-radius: var(--fbz-radius-control);
  border: none;
  background: var(--fbz-color-brand-500);
  color: #fff;
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  cursor: pointer;

  &:disabled {
    opacity: 0.55;
    cursor: default;
  }
}

.results {
  flex: 1;
  overflow: auto;
  padding: 0 var(--fbz-space-5) var(--fbz-space-5);
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);
}

.hint {
  margin: 0;
  padding: var(--fbz-space-3) 0;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
}

.candidate {
  display: flex;
  gap: var(--fbz-space-3);
  align-items: flex-start;
  text-align: left;
  padding: var(--fbz-space-3);
  border-radius: var(--fbz-radius-control);
  border: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel-strong);
  color: inherit;
  cursor: pointer;
  transition:
    border-color var(--fbz-motion-fast),
    background var(--fbz-motion-fast);

  &:hover:not(:disabled) {
    border-color: var(--fbz-color-brand-500);
  }

  &:disabled {
    opacity: 0.6;
    cursor: default;
  }
}

.thumb {
  width: 48px;
  height: 72px;
  object-fit: cover;
  border-radius: 4px;
  background: var(--fbz-color-panel);
  flex-shrink: 0;
}

.meta {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;

  strong {
    font-size: var(--fbz-font-size-md);
  }

  span {
    font-size: var(--fbz-font-size-sm);
    color: var(--fbz-color-text-muted);
  }

  small {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    color: var(--fbz-color-text-soft);
    font-size: var(--fbz-font-size-xs, 12px);
  }
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity var(--fbz-motion-fast);
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
