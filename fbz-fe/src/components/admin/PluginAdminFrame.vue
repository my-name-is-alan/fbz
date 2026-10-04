<script setup lang="ts">
import { request } from "@/service/request.ts";
const props = defineProps<{ pluginId: string; pagePath: string }>();
const emit = defineEmits<{ capabilities: [storageProvider: boolean] }>();
const frame = ref<HTMLIFrameElement>();
const documentHtml = ref("");
const allowed = ref<string[]>([]);
const hasUi = ref(false);
const loading = ref(true);
const error = ref("");
let generation = 0;
const csp =
  "default-src 'none'; base-uri 'none'; connect-src 'none'; form-action 'none'; frame-src 'none'; img-src data: blob:; style-src 'unsafe-inline'; script-src 'unsafe-inline'";
async function load() {
  const turn = ++generation;
  loading.value = true;
  error.value = "";
  hasUi.value = false;
  try {
    const response = await request.get<{
      html: string;
      actions: string[];
      storageProvider: boolean;
    }>(`/admin/plugins/${encodeURIComponent(props.pluginId)}/ui`);
    if (turn !== generation) return;
    allowed.value = response.data.actions;
    emit("capabilities", response.data.storageProvider);
    documentHtml.value = `<meta http-equiv="Content-Security-Policy" content="${csp}">${response.data.html}`;
    hasUi.value = true;
  } catch (e) {
    if (turn === generation) {
      const status = (e as { response?: { status?: number } })?.response?.status;
      emit("capabilities", false);
      if (status !== 404) error.value = "插件页面加载失败";
    }
  } finally {
    if (turn === generation) loading.value = false;
  }
}
watch(
  () => props.pluginId,
  () => void load(),
  { immediate: true },
);
async function onMessage(event: MessageEvent) {
  if (!frame.value || event.source !== frame.value.contentWindow || event.origin !== "null") return;
  const data = event.data as { type?: string; id?: unknown; action?: string; payload?: unknown };
  if (
    data?.type !== "fbz-plugin-action" ||
    typeof data.id !== "string" ||
    data.id.length > 64 ||
    typeof data.action !== "string" ||
    !allowed.value.includes(data.action)
  )
    return;
  if (data.payload != null && (typeof data.payload !== "object" || Array.isArray(data.payload)))
    return;
  let encoded = "";
  try {
    encoded = JSON.stringify(data.payload ?? {});
  } catch {
    return;
  }
  if (encoded.length > 32000) return;
  const target = event.source as Window;
  const turn = generation;
  const owner = props.pluginId;
  try {
    const result = await request.post(
      `/admin/plugins/${encodeURIComponent(props.pluginId)}/ui/actions/${encodeURIComponent(data.action)}`,
      { data: data.payload ?? {} },
    );
    if (
      turn === generation &&
      owner === props.pluginId &&
      event.source === frame.value?.contentWindow
    )
      target.postMessage({ type: "fbz-plugin-result", id: data.id, result: result.data }, "*");
  } catch {
    if (
      turn === generation &&
      owner === props.pluginId &&
      event.source === frame.value?.contentWindow
    )
      target.postMessage(
        { type: "fbz-plugin-result", id: data.id, error: "操作失败，请检查插件状态" },
        "*",
      );
  }
}
onMounted(() => window.addEventListener("message", onMessage));
onBeforeUnmount(() => {
  generation++;
  window.removeEventListener("message", onMessage);
});
watch(
  () => props.pagePath,
  (path) => {
    if (hasUi.value && frame.value?.contentWindow)
      frame.value.contentWindow.postMessage({ type: "fbz-plugin-page", path }, "*");
  },
);
function onFrameLoad() {
  frame.value?.contentWindow?.postMessage({ type: "fbz-plugin-page", path: props.pagePath }, "*");
}
</script>
<template>
  <p v-if="loading" role="status">正在加载插件页面…</p>
  <p v-else-if="error" role="alert">{{ error }}</p>
  <iframe
    v-else-if="hasUi"
    ref="frame"
    title="插件管理页面"
    sandbox="allow-scripts"
    :srcdoc="documentHtml"
    class="plugin-frame"
    @load="onFrameLoad"
  />
</template>
<style scoped lang="scss">
.plugin-frame {
  display: block;
  width: 100%;
  min-height: 600px;
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-card);
  background: var(--fbz-color-panel);
}
</style>
