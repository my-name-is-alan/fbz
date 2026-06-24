import type { RouteRecordRaw } from "vue-router";
import { createRouter, createWebHistory } from "vue-router";

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
      { path: "transcode", name: "admin-transcode", component: adminPage },
      // 系统设置
      { path: "users", name: "admin-users", component: adminPage },
      { path: "users/create", name: "admin-users-create", component: adminPage },
      { path: "users/:id", name: "admin-users-edit", component: adminPage },
      { path: "plugins", name: "admin-plugins", component: adminPage },
      { path: "metadata-mgr", name: "admin-metadata-mgr", component: adminPage },
      { path: "logs", name: "admin-logs", component: adminPage },
      { path: "about", name: "admin-about", component: adminPage },
    ],
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
