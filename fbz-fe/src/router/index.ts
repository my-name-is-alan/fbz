import type { RouteRecordRaw } from "vue-router";
import { createRouter, createWebHistory } from "vue-router";

export const routes = [
  {
    path: "/",
    name: "home",
    component: () => import("@/views/home/index.vue"),
  },
  {
    path: "/library",
    name: "library",
    component: () => import("@/views/library/index.vue"),
  },
  {
    path: "/library/:id",
    name: "library-detail",
    component: () => import("@/views/library/detail/index.vue"),
  },
  {
    path: "/movie/:id",
    name: "movie-detail",
    component: () => import("@/views/detail/movie/index.vue"),
  },
  {
    path: "/tv/:id",
    name: "tv-detail",
    component: () => import("@/views/detail/tv/index.vue"),
  },
  {
    path: "/person/:id",
    name: "person-detail",
    component: () => import("@/views/detail/person/index.vue"),
  },
  {
    path: "/collection/:id",
    name: "collection-detail",
    component: () => import("@/views/detail/collection/index.vue"),
  },
  {
    path: "/admin",
    name: "admin",
    component: () => import("@/views/account/index.vue"),
  },
  {
    path: "/profile",
    name: "profile",
    component: () => import("@/views/account/index.vue"),
  },
  {
    path: "/messages",
    name: "messages",
    component: () => import("@/views/account/index.vue"),
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
