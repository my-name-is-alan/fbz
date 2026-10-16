# Pages — dependency trees

Prioritized for admin console redesign. Trees include script imports + auto-imported Vue components (template tags).

Skip node_modules. Layout `src/layouts/admin.vue` wraps all `/admin/*` routes.

## Admin Layout (shared by all /admin/\*)

Entry: `src/layouts/admin.vue`
Dependencies:

- src/service/modules/admin.ts
  - src/service/request.ts
  - src/types/admin.ts
- src/stores/auth.ts
  - src/service/modules/admin.ts
  - src/service/modules/auth.ts
    - src/service/request.ts
    - src/types/navigation.ts
      - src/types/media.ts
  - src/service/request.ts
  - src/stores/ui.ts
    - src/service/modules/setup.ts
      - src/service/request.ts
      - src/types/setup.ts
    - src/types/media.ts
  - src/types/admin.ts
- src/stores/theme.ts
- src/types/admin.ts
- src/components/BaseAvatar.vue
  - src/service/request.ts

## /admin (控制面板 / Admin Dashboard)

Entry: `src/views/admin/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminDashboard.vue
  - src/service/modules/admin.ts
    - src/service/request.ts
    - src/types/admin.ts
  - src/stores/library.ts
    - src/service/modules/navigation.ts
      - src/service/request.ts
      - src/types/media.ts
      - src/types/navigation.ts
        - src/types/media.ts
    - src/types/admin.ts
    - src/types/media.ts
  - src/types/admin.ts

## /admin/users (用户管理)

Entry: `src/views/admin/users/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminUsers.vue
  - src/stores/auth.ts
    - src/service/modules/admin.ts
      - src/service/request.ts
      - src/types/admin.ts
    - src/service/modules/auth.ts
      - src/service/request.ts
      - src/types/navigation.ts
        - src/types/media.ts
    - src/service/request.ts
    - src/stores/ui.ts
      - src/service/modules/setup.ts
        - src/service/request.ts
        - src/types/setup.ts
      - src/types/media.ts
    - src/types/admin.ts
  - src/components/BaseAvatar.vue
    - src/service/request.ts

## /admin/users/create (添加用户)

Entry: `src/views/admin/users/create/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminUserEdit.vue
  - src/service/modules/admin.ts
    - src/service/request.ts
    - src/types/admin.ts
  - src/stores/auth.ts
    - src/service/modules/admin.ts
    - src/service/modules/auth.ts
      - src/service/request.ts
      - src/types/navigation.ts
        - src/types/media.ts
    - src/service/request.ts
    - src/stores/ui.ts
      - src/service/modules/setup.ts
        - src/service/request.ts
        - src/types/setup.ts
      - src/types/media.ts
    - src/types/admin.ts
  - src/stores/library.ts
    - src/service/modules/navigation.ts
      - src/service/request.ts
      - src/types/media.ts
      - src/types/navigation.ts
    - src/types/admin.ts
    - src/types/media.ts
  - src/stores/ui.ts
  - src/types/admin.ts
  - src/components/BaseSelect.vue

## /admin/users/:id (编辑用户)

Entry: `src/views/admin/users/edit/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminUserEdit.vue
  - src/service/modules/admin.ts
    - src/service/request.ts
    - src/types/admin.ts
  - src/stores/auth.ts
    - src/service/modules/admin.ts
    - src/service/modules/auth.ts
      - src/service/request.ts
      - src/types/navigation.ts
        - src/types/media.ts
    - src/service/request.ts
    - src/stores/ui.ts
      - src/service/modules/setup.ts
        - src/service/request.ts
        - src/types/setup.ts
      - src/types/media.ts
    - src/types/admin.ts
  - src/stores/library.ts
    - src/service/modules/navigation.ts
      - src/service/request.ts
      - src/types/media.ts
      - src/types/navigation.ts
    - src/types/admin.ts
    - src/types/media.ts
  - src/stores/ui.ts
  - src/types/admin.ts
  - src/components/BaseSelect.vue

## /admin/libraries (媒体库管理（含云库 + storage 相关）)

Entry: `src/views/admin/libraries/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminLibraries.vue
  - src/stores/library.ts
    - src/service/modules/navigation.ts
      - src/service/request.ts
      - src/types/media.ts
      - src/types/navigation.ts
        - src/types/media.ts
    - src/types/admin.ts
    - src/types/media.ts
  - src/stores/ui.ts
    - src/service/modules/setup.ts
      - src/service/request.ts
      - src/types/setup.ts
    - src/types/media.ts
  - src/components/admin/CloudLibraries.vue
    - src/service/modules/server.ts
      - src/service/request.ts
    - src/stores/library.ts
    - src/components/BaseIcon.vue
    - src/components/BaseModal.vue
      - src/components/BaseIcon.vue
    - src/components/BaseSelect.vue
    - src/components/admin/CloudLibraryPolicy.vue
      - src/components/BaseSelect.vue

## /admin/plugins (插件设置（含 CloudStoragePlugin / AdminStorage）)

Entry: `src/views/admin/plugins/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminPlugins.vue
  - src/service/modules/admin.ts
    - src/service/request.ts
    - src/types/admin.ts
  - src/stores/ui.ts
    - src/service/modules/setup.ts
      - src/service/request.ts
      - src/types/setup.ts
    - src/types/media.ts
  - src/types/admin.ts
  - src/components/admin/CloudStoragePlugin.vue
    - src/service/modules/server.ts
      - src/service/request.ts
    - src/components/BaseIcon.vue
    - src/components/BaseModal.vue
      - src/components/BaseIcon.vue
    - src/components/admin/AdminStorage.vue
      - src/service/modules/server.ts
      - src/stores/library.ts
        - src/service/modules/navigation.ts
          - src/service/request.ts
          - src/types/media.ts
          - src/types/navigation.ts
        - src/types/admin.ts
        - src/types/media.ts
      - src/components/BaseIcon.vue
      - src/components/BaseModal.vue
      - src/components/BaseEmptyState.vue
        - src/components/BaseIcon.vue

## /admin/plugins/:pluginId/config (插件配置)

Entry: `src/views/admin/plugins/config/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminPluginConfig.vue
  - src/service/modules/admin.ts
    - src/service/request.ts
    - src/types/admin.ts
  - src/stores/ui.ts
    - src/service/modules/setup.ts
      - src/service/request.ts
      - src/types/setup.ts
    - src/types/media.ts
  - src/types/admin.ts
  - src/components/admin/PluginConfigForm.vue
    - src/types/admin.ts
    - src/components/BaseSelect.vue

## /admin/transcode (转码设置)

Entry: `src/views/admin/transcode/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminTranscode.vue
  - src/service/modules/admin.ts
    - src/service/request.ts
    - src/types/admin.ts
  - src/stores/ui.ts
    - src/service/modules/setup.ts
      - src/service/request.ts
      - src/types/setup.ts
    - src/types/media.ts
  - src/types/admin.ts
  - src/components/BaseSelect.vue

## /admin/scheduled-tasks (任务计划)

Entry: `src/views/admin/scheduled-tasks/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminScheduledTasks.vue
  - src/service/modules/admin.ts
    - src/service/request.ts
    - src/types/admin.ts
  - src/stores/ui.ts
    - src/service/modules/setup.ts
      - src/service/request.ts
      - src/types/setup.ts
    - src/types/media.ts
  - src/types/admin.ts

## /admin/logs (系统日志)

Entry: `src/views/admin/logs/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminLogs.vue
  - src/service/request.ts
  - src/stores/ui.ts
    - src/service/modules/setup.ts
      - src/service/request.ts
      - src/types/setup.ts
    - src/types/media.ts

## /admin/theme (主题设置)

Entry: `src/views/admin/theme/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminTheme.vue
  - src/stores/theme.ts
  - src/stores/ui.ts
    - src/service/modules/setup.ts
      - src/service/request.ts
      - src/types/setup.ts
    - src/types/media.ts

## /admin/profile (个人信息)

Entry: `src/views/admin/profile/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminProfile.vue
  - src/service/modules/users.ts
    - src/service/request.ts
  - src/stores/auth.ts
    - src/service/modules/admin.ts
      - src/service/request.ts
      - src/types/admin.ts
    - src/service/modules/auth.ts
      - src/service/request.ts
      - src/types/navigation.ts
        - src/types/media.ts
    - src/service/request.ts
    - src/stores/ui.ts
      - src/service/modules/setup.ts
        - src/service/request.ts
        - src/types/setup.ts
      - src/types/media.ts
    - src/types/admin.ts
  - src/stores/ui.ts
  - src/components/BaseAvatar.vue
    - src/service/request.ts
  - src/components/BaseSelect.vue

## /admin/metadata (元数据设置)

Entry: `src/views/admin/metadata/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminMetadata.vue
  - src/service/modules/admin.ts
    - src/service/request.ts
    - src/types/admin.ts
  - src/stores/ui.ts
    - src/service/modules/setup.ts
      - src/service/request.ts
      - src/types/setup.ts
    - src/types/media.ts
  - src/components/BaseSelect.vue

## /admin/metadata-mgr (元数据管理)

Entry: `src/views/admin/metadata-mgr/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminMetadataMgr.vue
  - src/service/modules/admin.ts
    - src/service/request.ts
    - src/types/admin.ts
  - src/stores/ui.ts
    - src/service/modules/setup.ts
      - src/service/request.ts
      - src/types/setup.ts
    - src/types/media.ts

## /admin/plugin-market (插件市场)

Entry: `src/views/admin/plugin-market/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminPluginMarket.vue
  - src/service/modules/admin.ts
    - src/service/request.ts
    - src/types/admin.ts
  - src/stores/ui.ts
    - src/service/modules/setup.ts
      - src/service/request.ts
      - src/types/setup.ts
    - src/types/media.ts
  - src/types/admin.ts
  - src/components/BaseSelect.vue

## /admin/lib-sort (媒体库排序)

Entry: `src/views/admin/lib-sort/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminLibSort.vue
  - src/stores/library.ts
    - src/service/modules/navigation.ts
      - src/service/request.ts
      - src/types/media.ts
      - src/types/navigation.ts
        - src/types/media.ts
    - src/types/admin.ts
    - src/types/media.ts
  - src/stores/ui.ts
    - src/service/modules/setup.ts
      - src/service/request.ts
      - src/types/setup.ts
    - src/types/media.ts
  - src/components/BaseIcon.vue

## /admin/photos (家庭相册)

Entry: `src/views/admin/photos/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminPhotos.vue
  - src/service/modules/admin.ts
    - src/service/request.ts
    - src/types/admin.ts
  - src/types/admin.ts
  - src/components/BaseSelect.vue

## /admin/about (关于)

Entry: `src/views/admin/about/index.vue`
Dependencies:

- src/components/admin/AdminPageShell.vue
- src/components/admin/AdminAbout.vue
  - src/service/modules/admin.ts
    - src/service/request.ts
    - src/types/admin.ts
  - src/types/admin.ts

## / (前台首页（对照）)

Entry: `src/views/home/index.vue`
Dependencies:

- src/service/modules/navigation.ts
  - src/service/request.ts
  - src/types/media.ts
  - src/types/navigation.ts
    - src/types/media.ts
- src/stores/library.ts
  - src/service/modules/navigation.ts
  - src/types/admin.ts
  - src/types/media.ts
- src/stores/ui.ts
  - src/service/modules/setup.ts
    - src/service/request.ts
    - src/types/setup.ts
  - src/types/media.ts
- src/types/media.ts
- src/types/navigation.ts
- src/components/home/HomeHero.vue
  - src/stores/playback.ts
    - src/service/modules/server.ts
      - src/service/request.ts
    - src/stores/ui.ts
  - src/types/media.ts
- src/components/media/MediaRow.vue
  - src/types/media.ts
  - src/components/BaseScroller.vue
    - src/components/BaseScroller.vue
  - src/components/media/MediaCard.vue
    - src/stores/playback.ts
    - src/stores/ui.ts
    - src/types/media.ts
    - src/components/media/MediaPoster.vue
      - src/service/request.ts
