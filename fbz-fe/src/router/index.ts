import type { RouteRecordRaw } from "vue-router";
import { createRouter, createWebHistory } from "vue-router";

import { setupRouterGuard } from "@/router/guard.ts";

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
      {
        path: "",
        name: "admin-dashboard",
        component: () => import("@/views/admin/index.vue"),
      },
      // 个人偏好
      {
        path: "profile",
        name: "admin-profile",
        component: () => import("@/views/admin/profile/index.vue"),
      },
      {
        path: "theme",
        name: "admin-theme",
        component: () => import("@/views/admin/theme/index.vue"),
      },
      {
        path: "lib-sort",
        name: "admin-lib-sort",
        component: () => import("@/views/admin/lib-sort/index.vue"),
      },
      // 媒体设置
      {
        path: "metadata",
        name: "admin-metadata",
        component: () => import("@/views/admin/metadata/index.vue"),
      },
      {
        path: "libraries",
        name: "admin-libraries",
        component: () => import("@/views/admin/libraries/index.vue"),
      },
      {
        path: "photos",
        name: "admin-photos",
        component: () => import("@/views/admin/photos/index.vue"),
      },
      {
        path: "transcode",
        name: "admin-transcode",
        component: () => import("@/views/admin/transcode/index.vue"),
      },
      // 系统设置
      {
        path: "users",
        name: "admin-users",
        component: () => import("@/views/admin/users/index.vue"),
      },
      {
        path: "users/create",
        name: "admin-users-create",
        component: () => import("@/views/admin/users/create/index.vue"),
      },
      {
        path: "users/:id",
        name: "admin-users-edit",
        component: () => import("@/views/admin/users/edit/index.vue"),
      },
      {
        path: "plugins",
        name: "admin-plugins",
        component: () => import("@/views/admin/plugins/index.vue"),
      },
      {
        path: "plugin-market",
        name: "admin-plugin-market",
        component: () => import("@/views/admin/plugin-market/index.vue"),
      },
      // 独立配置页须写在 plugins/:pluginId/:menuPath 之前，避免被吞
      {
        path: "plugins/:pluginId/config",
        name: "admin-plugin-config",
        component: () => import("@/views/admin/plugins/config/index.vue"),
      },
      // 插件声明的管理菜单页（manifest menu，路径命名空间 /admin/plugins/{pluginId}/...）
      {
        path: "plugins/:pluginId/:menuPath(.*)*",
        name: "admin-plugin-page",
        component: () => import("@/views/admin/plugins/page/index.vue"),
      },
      {
        path: "scheduled-tasks",
        name: "admin-scheduled-tasks",
        component: () => import("@/views/admin/scheduled-tasks/index.vue"),
      },
      {
        path: "metadata-mgr",
        name: "admin-metadata-mgr",
        component: () => import("@/views/admin/metadata-mgr/index.vue"),
      },
      {
        path: "logs",
        name: "admin-logs",
        component: () => import("@/views/admin/logs/index.vue"),
      },
      {
        path: "about",
        name: "admin-about",
        component: () => import("@/views/admin/about/index.vue"),
      },
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
