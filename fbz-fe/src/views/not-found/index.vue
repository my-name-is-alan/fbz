<script setup lang="ts">
import { useCSurface } from "@/composables/useCSurface.ts";

useCSurface();
</script>

<template>
  <main class="not-found-view">
    <span class="code-ic"><BaseIcon name="compass" :size="30" :stroke-width="1.5" /></span>
    <p class="code">404</p>
    <h1>页面不存在</h1>
    <p class="desc">链接可能已失效，或该条目已被移除。</p>
    <RouterLink class="du-btn du-btn-primary" to="/"
      ><BaseIcon name="home" :size="16" />返回首页</RouterLink
    >
  </main>
</template>

<style scoped lang="scss">
.not-found-view {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 24px;
  background: var(--ui-background);
  color: var(--ui-text);
  text-align: center;
}

.code-ic {
  width: 64px;
  height: 64px;
  display: grid;
  place-items: center;
  margin-bottom: 8px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  background: var(--ui-surface);
  color: var(--ui-accent);
}

.code {
  margin: 0;
  color: var(--ui-text-subtle);
  font-family: var(--ui-font-mono);
  font-size: 13px;
  letter-spacing: 0.2em;
}

h1 {
  margin: 0;
  font-size: 22px;
  font-weight: 600;
}

.desc {
  margin: 0 0 12px;
  color: var(--ui-text-muted);
  font-size: 13px;
}
</style>
