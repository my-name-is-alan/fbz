<script setup lang="ts">
import { serverRequest, errorMessage } from "@/service/modules/server.ts";
const configuring = ref(false);
const configured = ref(false);
const count = ref(0);
const error = ref("");
async function refresh() {
  try {
    const data = await serverRequest<{ configured: boolean; accounts: unknown[] }>(
      "/api/admin/storage",
    );
    configured.value = data.configured;
    count.value = data.accounts.filter((a: unknown) => {
      const record = a as { provider?: string };
      return !record.provider || record.provider === "guangya";
    }).length;
  } catch (e) {
    error.value = errorMessage(e);
  }
}
onMounted(refresh);
</script>
<template>
  <section class="plugins-page">
    <p v-if="error" class="du-alert du-alert-error">{{ error }}</p>
    <div v-if="count" class="plugin-section">
      <h2>存储插件</h2>
      <span>1 个可用集成</span>
    </div>
    <article v-if="count" class="plugin-card">
      <span class="plugin-icon"><BaseIcon name="cloud" :size="28" /></span>
      <div class="plugin-description">
        <h3>旧版光鸭连接 <span class="du-badge du-badge-sm">存储源</span></h3>
        <p>扫码连接账号，将云端目录挂载到 FBZ，再通过媒体库添加内容。</p>
        <small>{{ configured ? `已配置 · ${count} 个账号` : "服务端尚未配置" }} · 只读访问</small>
      </div>
      <button class="du-btn du-btn-sm" @click="configuring = true">配置插件</button>
    </article>
    <p v-if="count" class="plugin-note">
      NFO 来源与图片缓存策略在媒体库设置中配置。插件配置不会自动创建媒体库。
    </p>
    <BaseModal
      :open="configuring"
      wide
      title="光鸭网盘"
      description="管理账号与虚拟挂载目录"
      @close="
        configuring = false;
        refresh();
      "
      ><AdminStorage v-if="configuring"
    /></BaseModal>
  </section>
</template>
<style scoped>
.plugin-section {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}
.plugin-section h2 {
  font-size: 18px;
  font-weight: 600;
}
.plugin-section span,
.plugin-note {
  font-size: 12px;
  color: var(--fbz-color-text-muted);
}
.plugin-card {
  display: flex;
  gap: 20px;
  align-items: center;
  padding: 24px;
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel);
  border-radius: 12px;
}
.plugin-icon {
  display: grid;
  place-items: center;
  width: 56px;
  height: 56px;
  border-radius: 12px;
  background: var(--fbz-color-panel-strong);
  flex-shrink: 0;
}
.plugin-description {
  flex: 1;
}
.plugin-description h3 {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 17px;
  font-weight: 600;
  margin-bottom: 10px;
}
.plugin-description p {
  font-size: 13px;
  line-height: 1.8;
  color: var(--fbz-color-text-soft);
}
.plugin-description small {
  display: block;
  margin-top: 12px;
  color: var(--fbz-color-text-muted);
}
.plugin-note {
  margin-top: 20px;
  line-height: 1.8;
}
@media (max-width: 600px) {
  .plugin-card {
    flex-wrap: wrap;
  }
  .plugin-description {
    min-width: 180px;
  }
  .plugin-card > .du-btn {
    margin-left: auto;
  }
}
</style>
