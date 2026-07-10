import type { RouteRecordRaw } from "vue-router";
import { createRouter, createWebHistory } from "vue-router";

import { setupRouterGuard } from "@/router/guard.ts";

const adminPage = () => import("@/views/account/index.vue");

export const routes = [
  {
    path: "/",
    component: () => import("@/layouts/default.vue"),
    children: [
      {
        path: "",
        name: "home",
        component: () => import("@/views/home/index.vue"),
      },
      {
        path: "search",
        name: "search",
        component: () => import("@/views/search/index.vue"),
      },
      {
        path: "library",
        name: "library",
        component: () => import("@/views/library/index.vue"),
      },
      {
        path: "library/:id",
        name: "library-detail",
        component: () => import("@/views/library/detail/index.vue"),
      },
      {
        path: "music/:id",
        name: "music-library",
        component: () => import("@/views/music/library/index.vue"),
      },
      {
        path: "artist/:id",
        name: "artist-detail",
        component: () => import("@/views/detail/artist/index.vue"),
      },
      {
        path: "album/:id",
        name: "album-detail",
        component: () => import("@/views/detail/album/index.vue"),
      },
      {
        path: "movie/:id",
        name: "movie-detail",
        component: () => import("@/views/detail/movie/index.vue"),
      },
      {
        path: "tv/:id",
        name: "tv-detail",
        component: () => import("@/views/detail/tv/index.vue"),
      },
      {
        path: "person/:id",
        name: "person-detail",
        component: () => import("@/views/detail/person/index.vue"),
      },
      {
        path: "collection/:id",
        name: "collection-detail",
        component: () => import("@/views/detail/collection/index.vue"),
      },
    ],
  },
  {
    path: "/admin",
    component: () => import("@/layouts/admin.vue"),
    children: [
      // 控制面板
      { path: "", name: "admin-dashboard", component: adminPage },
      // 个人偏好
      { path: "profile", name: "admin-profile", component: adminPage },
      { path: "theme", name: "admin-theme", component: adminPage },
      { path: "lib-sort", name: "admin-lib-sort", component: adminPage },
      // 媒体设置
      { path: "metadata", name: "admin-metadata", component: adminPage },
      { path: "libraries", name: "admin-libraries", component: adminPage },
      { path: "photos", name: "admin-photos", component: adminPage },
      { path: "transcode", name: "admin-transcode", component: adminPage },
      // 系统设置
      { path: "users", name: "admin-users", component: adminPage },
      { path: "users/create", name: "admin-users-create", component: adminPage },
      { path: "users/:id", name: "admin-users-edit", component: adminPage },
      { path: "plugins", name: "admin-plugins", component: adminPage },
      { path: "plugin-market", name: "admin-plugin-market", component: adminPage },
      // 独立配置页须写在 plugins/:pluginId/:menuPath 之前，避免被吞
      {
        path: "plugins/:pluginId/config",
        name: "admin-plugin-config",
        component: adminPage,
      },
      // 插件声明的管理菜单页（manifest menu，路径命名空间 /admin/plugins/{pluginId}/...）
      {
        path: "plugins/:pluginId/:menuPath(.*)*",
        name: "admin-plugin-page",
        component: adminPage,
      },
      { path: "scheduled-tasks", name: "admin-scheduled-tasks", component: adminPage },
      { path: "metadata-mgr", name: "admin-metadata-mgr", component: adminPage },
      { path: "logs", name: "admin-logs", component: adminPage },
      { path: "about", name: "admin-about", component: adminPage },
    ],
  },
  {
    path: "/user/login",
    name: "login",
    component: () => import("@/views/user/login/index.vue"),
  },
  {
    path: "/:pathMatch(.*)*",
    name: "not-found",
    component: () => import("@/views/not-found/index.vue"),
  },
] satisfies RouteRecordRaw[];

export const router = createRouter({
  history: createWebHistory(),
  routes,
  scrollBehavior(_to, _from, savedPosition) {
    return savedPosition ?? { top: 0 };
  },
});

setupRouterGuard(router);
