# Routes

Config-based Vue Router (`createWebHistory`). Source: `src/router/index.ts`.

Layouts: `/` children → `layouts/default.vue`; `/admin` children → `layouts/admin.vue`; login & 404 are layout-less.

## Framework detection

- Framework: Vue 3.5 + Vue Router 5

- Build: Vite (vite-plus / `vp`)

- CSS: SCSS tokens + UnoCSS (presetWind3) + TailwindCSS 4 + daisyUI 5 (`du-` prefix)

- State: Pinia

## FULL router config

```ts
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
      { path: "storage", redirect: "/admin/plugins" },
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
```

## Route table

| Path               | Name                | Component                               | Layout  | Notes                    |
| ------------------ | ------------------- | --------------------------------------- | ------- | ------------------------ |
| `/`                | `home`              | `src/views/home/index.vue`              | default | Home / continue watching |
| `/search`          | `search`            | `src/views/search/index.vue`            | default | Search                   |
| `/library`         | `library`           | `src/views/library/index.vue`           | default | Library overview         |
| `/library/:id`     | `library-detail`    | `src/views/library/detail/index.vue`    | default | Library detail           |
| `/music/:id`       | `music-library`     | `src/views/music/library/index.vue`     | default | Music library            |
| `/artist/:id`      | `artist-detail`     | `src/views/detail/artist/index.vue`     | default |                          |
| `/album/:id`       | `album-detail`      | `src/views/detail/album/index.vue`      | default |                          |
| `/movie/:id`       | `movie-detail`      | `src/views/detail/movie/index.vue`      | default |                          |
| `/tv/:id`          | `tv-detail`         | `src/views/detail/tv/index.vue`         | default |                          |
| `/person/:id`      | `person-detail`     | `src/views/detail/person/index.vue`     | default |                          |
| `/collection/:id`  | `collection-detail` | `src/views/detail/collection/index.vue` | default |                          |
| `/user/login`      | `login`             | `src/views/user/login/index.vue`        | (none)  | Login                    |
| `/:pathMatch(.*)*` | `not-found`         | `src/views/not-found/index.vue`         | (none)  | 404                      |

## ★ Admin routes (重点)

| Path                                      | Name                    | Component                                   | Nav group    | Summary                                              |
| ----------------------------------------- | ----------------------- | ------------------------------------------- | ------------ | ---------------------------------------------------- |
| `/admin`                                  | `admin-dashboard`       | `src/views/admin/index.vue`                 | 控制面板     | Jobs / scheduled tasks / library metrics dashboard   |
| `/admin/profile`                          | `admin-profile`         | `src/views/admin/profile/index.vue`         | 个人偏好     | Account nickname/password/language                   |
| `/admin/theme`                            | `admin-theme`           | `src/views/admin/theme/index.vue`           | 个人偏好     | Dark/light + brand color pickers                     |
| `/admin/lib-sort`                         | `admin-lib-sort`        | `src/views/admin/lib-sort/index.vue`        | 个人偏好     | Drag/reorder libraries for front-end nav             |
| `/admin/metadata`                         | `admin-metadata`        | `src/views/admin/metadata/index.vue`        | 媒体设置     | Scraper priority / metadata language                 |
| `/admin/libraries`                        | `admin-libraries`       | `src/views/admin/libraries/index.vue`       | 媒体设置     | Local + cloud library cards; storage redirect target |
| `/admin/photos`                           | `admin-photos`          | `src/views/admin/photos/index.vue`          | (route only) | Family photo timeline (not in sidebar nav)           |
| `/admin/transcode`                        | `admin-transcode`       | `src/views/admin/transcode/index.vue`       | 媒体设置     | HW accel / encoder / quality toggles                 |
| `/admin/users`                            | `admin-users`           | `src/views/admin/users/index.vue`           | 系统设置     | User card grid                                       |
| `/admin/users/create`                     | `admin-users-create`    | `src/views/admin/users/create/index.vue`    | 系统设置     | Create user form (AdminUserEdit)                     |
| `/admin/users/:id`                        | `admin-users-edit`      | `src/views/admin/users/edit/index.vue`      | 系统设置     | Edit user form (AdminUserEdit)                       |
| `/admin/plugins`                          | `admin-plugins`         | `src/views/admin/plugins/index.vue`         | 系统设置     | Plugins / packages / dispatches + cloud storage      |
| `/admin/plugin-market`                    | `admin-plugin-market`   | `src/views/admin/plugin-market/index.vue`   | 系统设置     | Remote plugin marketplace                            |
| `/admin/plugins/:pluginId/config`         | `admin-plugin-config`   | `src/views/admin/plugins/config/index.vue`  | 系统设置     | Plugin config form                                   |
| `/admin/plugins/:pluginId/:menuPath(.*)*` | `admin-plugin-page`     | `src/views/admin/plugins/page/index.vue`    | 插件菜单     | Plugin-declared admin menu page                      |
| `/admin/scheduled-tasks`                  | `admin-scheduled-tasks` | `src/views/admin/scheduled-tasks/index.vue` | 系统设置     | Scheduler list + run history                         |
| `/admin/metadata-mgr`                     | `admin-metadata-mgr`    | `src/views/admin/metadata-mgr/index.vue`    | 系统设置     | Batch metadata repair                                |
| `/admin/logs`                             | `admin-logs`            | `src/views/admin/logs/index.vue`            | 系统设置     | Terminal-style log console                           |
| `/admin/about`                            | `admin-about`           | `src/views/admin/about/index.vue`           | 系统设置     | Version / license                                    |
| `/admin/storage`                          | `(redirect)`            | `→ /admin/plugins`                          | —            | Legacy storage path redirects to plugins             |

### Admin sidebar nav groups (from `layouts/admin.vue`)

1. **控制面板** → `/admin`

2. **个人偏好** → profile / theme / lib-sort

3. **媒体设置** → metadata / libraries / transcode

4. **系统设置** → users / plugins / plugin-market / scheduled-tasks / metadata-mgr / logs / about

5. **插件菜单** → dynamic from `listPluginMenuItems()`

6. Back link → `/`
