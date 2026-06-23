<script setup lang="ts">
const route = useRoute();

const pageMap = {
  admin: {
    title: "后台管理",
    desc: "集中处理媒体库同步、转码队列、账号权限和系统任务。",
  },
  profile: {
    title: "个人信息",
    desc: "查看账号资料、播放偏好、设备和会话状态。",
  },
  messages: {
    title: "消息中心",
    desc: "接收媒体入库、播放失败、后台任务和系统提醒。",
  },
} as const;

const page = computed(() => {
  const name = String(route.name ?? "profile");
  return pageMap[name as keyof typeof pageMap] ?? pageMap.profile;
});
</script>

<template>
  <main class="account-view">
    <PageHeader :title="page.title" />

    <section class="account-panel">
      <p class="eyebrow">账户</p>
      <h1>{{ page.title }}</h1>
      <p class="desc">{{ page.desc }}</p>

      <div class="state">
        <span class="state-dot" />
        <span>功能入口已接入，等待后端账号模块联调。</span>
      </div>
    </section>
  </main>
</template>

<style scoped lang="scss">
.account-view {
  min-height: 100vh;
  padding: calc(var(--header-h, 60px) + var(--fbz-space-8)) var(--fbz-space-8) 80px;
}

.account-panel {
  max-width: 760px;
  margin: 0 auto;
  padding: var(--fbz-space-6);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-card);
  background: rgba(20, 20, 22, 0.72);
}

.eyebrow {
  margin: 0 0 var(--fbz-space-2);
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
}

h1 {
  margin: 0;
  font-size: var(--fbz-font-size-xl);
  font-weight: 900;
}

.desc {
  max-width: 560px;
  margin: var(--fbz-space-3) 0 var(--fbz-space-5);
  color: var(--fbz-color-text-soft);
  font-size: var(--fbz-font-size-md);
  line-height: 1.7;
}

.state {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-2);
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
}

.state-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--fbz-color-brand-500);
  box-shadow: 0 0 0 4px rgba(30, 215, 96, 0.12);
}

@media (max-width: 600px) {
  .account-view {
    padding: calc(var(--header-h, 56px) + var(--fbz-space-5)) var(--fbz-space-4) 60px;
  }

  .account-panel {
    padding: var(--fbz-space-4);
  }
}
</style>
