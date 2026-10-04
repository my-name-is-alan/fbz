<script setup lang="ts">
const route = useRoute();
const drawer = ref(false);
const groups = [
  {
    label: "服务管理",
    items: [
      { label: "仪表盘", to: "/admin", name: "admin-dashboard", icon: "home" },
      { label: "媒体库", to: "/admin/libraries", name: "admin-libraries", icon: "library" },
      { label: "插件", to: "/admin/plugins", name: "admin-plugins", icon: "grid" },
      { label: "用户与权限", to: "/admin/users", name: "admin-users", icon: "user" },
    ],
  },
  {
    label: "系统设置",
    items: [
      { label: "任务与日志", to: "/admin/logs", name: "admin-logs", icon: "list" },
      { label: "转码设置", to: "/admin/transcode", name: "admin-transcode", icon: "settings" },
      { label: "元数据设置", to: "/admin/metadata", name: "admin-metadata", icon: "movie" },
      {
        label: "元数据管理",
        to: "/admin/metadata-mgr",
        name: "admin-metadata-mgr",
        icon: "folder",
      },
    ],
  },
  {
    label: "个人偏好",
    items: [
      { label: "个人信息", to: "/admin/profile", name: "admin-profile", icon: "user" },
      { label: "主题外观", to: "/admin/theme", name: "admin-theme", icon: "sun" },
      { label: "媒体库排序", to: "/admin/lib-sort", name: "admin-lib-sort", icon: "filter" },
      { label: "关于 FBZ", to: "/admin/about", name: "admin-about", icon: "info" },
    ],
  },
];
function active(name: string) {
  return (
    route.name === name || (name === "admin-users" && String(route.name).startsWith("admin-users"))
  );
}
</script>
<template>
  <div class="admin-shell">
    <AppHeader />
    <div class="admin-frame">
      <aside class="admin-sidebar">
        <div class="console-identity">
          <span><BaseIcon name="settings" :size="21" /></span>
          <div><strong>FBZ 控制台</strong><small>服务器管理</small></div>
        </div>
        <nav aria-label="后台管理导航">
          <div v-for="group in groups" :key="group.label" class="navigation-group">
            <p>{{ group.label }}</p>
            <ul class="du-menu">
              <li v-for="item in group.items" :key="item.name">
                <RouterLink :to="item.to" :class="{ 'du-menu-active': active(item.name) }"
                  ><BaseIcon :name="item.icon" :size="17" />{{ item.label }}</RouterLink
                >
              </li>
            </ul>
          </div>
        </nav>
        <footer><span>FBZ</span><small>v0.1.0</small></footer>
      </aside>
      <div class="admin-main">
        <div class="mobile-console-bar">
          <button class="du-btn du-btn-ghost du-btn-sm" @click="drawer = true">
            <BaseIcon name="menu" :size="18" />管理导航</button
          ><span>控制台</span>
        </div>
        <RouterView />
      </div>
    </div>
    <BaseModal :open="drawer" title="管理控制台" @close="drawer = false"
      ><nav class="mobile-admin-menu" aria-label="后台移动端导航">
        <ul v-for="group in groups" :key="group.label" class="du-menu">
          <li class="du-menu-title">{{ group.label }}</li>
          <li v-for="item in group.items" :key="item.name">
            <RouterLink
              :to="item.to"
              :class="{ 'du-menu-active': active(item.name) }"
              @click="drawer = false"
              ><BaseIcon :name="item.icon" :size="17" />{{ item.label }}</RouterLink
            >
          </li>
        </ul>
      </nav></BaseModal
    >
  </div>
</template>
<style scoped lang="scss">
.admin-shell {
  min-height: 100dvh;
  background: var(--fbz-color-bg);
  color: var(--fbz-color-text);
}
.admin-frame {
  max-width: 1700px;
  margin: auto;
  display: grid;
  grid-template-columns: 220px minmax(0, 1fr);
  gap: 64px;
  padding: calc(var(--header-h) + 48px) 4.5vw 70px;
}
.admin-sidebar {
  position: sticky;
  top: calc(var(--header-h) + 25px);
  height: calc(100dvh - var(--header-h) - 55px);
  display: flex;
  flex-direction: column;
}
.console-identity {
  display: flex;
  gap: 12px;
  align-items: center;
  padding: 0 10px 25px;
}
.console-identity > span {
  display: grid;
  place-items: center;
  width: 36px;
  height: 36px;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 11px;
  color: var(--fbz-color-text-muted);
}
.console-identity strong {
  display: block;
  font-size: 15px;
  font-weight: 600;
}
.console-identity small {
  display: block;
  font-size: 13px;
  color: var(--fbz-color-text-muted);
  margin-top: 4px;
}
.admin-sidebar nav {
  overflow: auto;
  flex: 1;
}
.navigation-group > p {
  font-size: 13px;
  color: var(--fbz-color-text-muted);
  letter-spacing: 0;
  padding: 0 12px;
  margin: 16px 0 8px;
}
.du-menu {
  width: 100%;
  padding: 0;
  gap: 2px;
}
.du-menu a {
  font-size: 13px;
  font-weight: 450;
  padding: 10px 14px;
  gap: 13px;
  border: 1px solid transparent;
  border-radius: 8px;
  color: var(--fbz-color-text-soft);
}
.du-menu a svg {
  color: var(--fbz-color-text-muted);
}
.du-menu a.du-menu-active {
  background: var(--fbz-color-panel-strong);
  color: var(--fbz-color-text);
  border-color: var(--fbz-color-line-soft);
}
.du-menu a.du-menu-active svg {
  color: var(--fbz-color-text);
}
footer {
  display: flex;
  justify-content: space-between;
  padding: 20px 13px 0;
  color: var(--fbz-color-text-muted);
  font-size: 13px;
  letter-spacing: 1px;
}
.admin-main {
  min-width: 0;
  padding-top: 3px;
}
.mobile-console-bar {
  display: none;
}
.admin-main :deep(.account-view) {
  padding: 0;
}
.admin-main :deep(.panel-header-banner) {
  padding: 0;
  margin-bottom: 30px;
  background: transparent;
  border: 0;
  box-shadow: none;
}
.admin-main :deep(.page-heading) {
  font-size: 26px;
  line-height: 1.25;
  font-weight: 550;
  letter-spacing: -0.8px;
  margin: 0 0 10px;
}
.admin-main :deep(.description-text) {
  font-size: 13px;
  color: var(--fbz-color-text-muted);
  line-height: 1.8;
}
.admin-main :deep(.du-btn) {
  font-family: var(--fbz-font-sans);
  font-size: 13px;
  letter-spacing: 0;
  min-height: 34px;
  border-radius: 8px;
  box-shadow: none;
}
.admin-main :deep(.du-btn-primary) {
  background: var(--fbz-color-brand-500);
  color: #07150e;
  border-color: transparent;
}
.admin-main :deep(.du-input),
.admin-main :deep(.du-textarea) {
  font-family: var(--fbz-font-sans);
  font-size: 13px;
  background: var(--fbz-color-panel);
  border-color: var(--fbz-color-line);
  border-radius: 8px;
  color: var(--fbz-color-text);
}
.admin-main :deep(.du-select) {
  font-size: 13px;
}
.mobile-admin-menu {
  max-height: 65dvh;
  overflow: auto;
}
.mobile-admin-menu .du-menu {
  margin-bottom: 20px;
}
@media (max-width: 1100px) {
  .admin-frame {
    grid-template-columns: 200px minmax(0, 1fr);
    gap: 28px;
    padding-inline: 26px;
  }
}
@media (max-width: 800px) {
  .admin-frame {
    display: block;
    padding: calc(var(--header-h) + 20px) 22px 100px;
  }
  .admin-sidebar {
    display: none;
  }
  .mobile-console-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 28px;
    color: var(--fbz-color-text-muted);
    font-size: 11px;
  }
  .mobile-console-bar .du-btn {
    padding-left: 0;
  }
  .admin-main :deep(.page-heading) {
    font-size: 25px;
  }
}

.admin-main :deep(.panel-header-banner) {
  padding-bottom: 26px;
  border-bottom: 1px solid var(--fbz-color-line-soft);
  margin-bottom: 30px;
}
.admin-main :deep(.description-text) {
  font-size: 13px;
}
.admin-main :deep(.du-btn-primary) {
  background: var(--fbz-color-text);
  color: var(--fbz-color-bg);
}
.admin-sidebar footer {
  border-top: 1px solid var(--fbz-color-line-soft);
  margin-top: 20px;
}
@media (min-width: 801px) {
  .admin-sidebar {
    top: calc(var(--header-h) + 32px);
  }
}
</style>
