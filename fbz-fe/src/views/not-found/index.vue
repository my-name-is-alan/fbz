<template>
  <main class="not-found-view">
    <p class="not-found-view__eyebrow">404</p>
    <h1>页面不存在</h1>
    <RouterLink class="not-found-view__link" to="/"> 返回首页 </RouterLink>
  </main>
</template>

<style scoped lang="scss">
.not-found-view {
  min-height: 100vh;
  display: grid;
  place-content: center;
  gap: 16px;
  color: var(--fbz-color-text);
  background: var(--fbz-color-bg);
  text-align: center;
}

.not-found-view__eyebrow {
  color: var(--fbz-color-cyan-500);
  font-weight: 800;
  margin: 0;
}

.not-found-view h1 {
  font-size: 32px;
  margin: 0;
}

.not-found-view__link {
  color: var(--fbz-color-cyan-500);
  text-decoration: none;
  font-size: 14px;
}
</style>
