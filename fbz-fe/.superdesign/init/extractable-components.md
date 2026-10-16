# Extractable Components (DraftComponent candidates)

Components reusable across pages. Full source in `components.md` / `layouts.md` — this file is the extraction menu only.

## Layout Components

## AdminShell

- Source: `src/layouts/admin.vue`
- Category: layout
- Description: Admin console shell: sidebar nav groups, mobile drawer, sticky top bar, theme toggle, user badge.
- Extractable props: activeRouteName (string), themeMode ("light"|"dark"), nickname (string), pluginMenuItems (array)
- Hardcoded: FBZ brand logo text, nav group labels/icons SVG paths, version string Fbz v0.1.0, all SCSS

## AppHeader

- Source: `src/components/layout/AppHeader.vue`
- Category: layout
- Description: Front-end fixed header with brand, library mega-menu, search, account dropdown.
- Extractable props: activeItem (string, default "home"), libraries (array), nickname (string), searchTerm (string), hasHeroBackdrop (boolean)
- Hardcoded: Brand FBZ, nav labels 首页/媒体库/最近添加, accountLinks paths, emoji/hamburger glyphs, all SCSS

## AppDrawer

- Source: `src/components/layout/AppDrawer.vue`
- Category: layout
- Description: Mobile navigation drawer for front-end routes.
- Extractable props: open (boolean v-model), libraries (array)
- Hardcoded: Link labels, drawer width 76%/320px, mask opacity, all SCSS

## DefaultShell

- Source: `src/layouts/default.vue`
- Category: layout
- Description: Front-end app shell composing header, drawer, router-view, music player bar.
- Extractable props: drawerOpen (boolean)
- Hardcoded: Composition only; child component styles

## AdminPageShell

- Source: `src/components/admin/AdminPageShell.vue`
- Category: layout
- Description: Admin page content wrapper with optional title/description banner.
- Extractable props: title (string?), description (string?)
- Hardcoded: max-width 1200px, banner typography, all SCSS

## MusicPlayerBar

- Source: `src/components/playback/MusicPlayerBar.vue`
- Category: layout
- Description: Bottom persistent music player bar on front-end layout.
- Extractable props: playing (boolean), trackTitle (string), progress (number)
- Hardcoded: Player chrome icons/layout CSS

## Basic Components

## BaseAvatar

- Source: `src/components/BaseAvatar.vue`
- Category: basic
- Description: User avatar with image URL or hashed-color initial fallback.
- Extractable props: userId (string?), name (string), size (number, default 34), version (string|number?)
- Hardcoded: Round clip, font weight, white letter color

## BaseEmptyState

- Source: `src/components/BaseEmptyState.vue`
- Category: basic
- Description: Dashed empty panel with icon, title, description, action slot.
- Extractable props: icon (string), title (string), description (string?), compact (boolean)
- Hardcoded: Default emoji 📁, dashed border, padding values

## BaseIcon

- Source: `src/components/BaseIcon.vue`
- Category: basic
- Description: Named Lucide icon registry.
- Extractable props: name (string), size (number, default 20)
- Hardcoded: Icon name→component map, stroke-width 1.7

## BaseModal

- Source: `src/components/BaseModal.vue`
- Category: basic
- Description: Centered/bottom-sheet dialog using native dialog + daisyUI modal box.
- Extractable props: open (boolean), title (string), description (string?), wide (boolean)
- Hardcoded: du-modal classes, blur backdrop, widths 580/920, close icon

## BaseSelect

- Source: `src/components/BaseSelect.vue`
- Category: basic
- Description: Styled select with typed options and v-model.
- Extractable props: modelValue (T), options ({label,value}[]), size ("sm"|"md"), ariaLabel?, placeholder?
- Hardcoded: du-select classes, font-size 13

## BaseScroller

- Source: `src/components/BaseScroller.vue`
- Category: basic
- Description: Horizontal card scroller with gradient edge arrows.
- Extractable props: colWidth (string), gap (string), step (number)
- Hardcoded: Arrow SVG paths, gradient math, touch hide rule

## GlobalToast

- Source: `src/components/GlobalToast.vue`
- Category: basic
- Description: Stacked toast notifications from ui store.
- Extractable props: toasts (array from store: id, type, message)
- Hardcoded: Emoji icons per type, fixed top-right position, transition curves

## MediaCard

- Source: `src/components/media/MediaCard.vue`
- Category: basic
- Description: Poster card for movies/shows used in rows and grids.
- Extractable props: item (MediaCardItem), showProgress (boolean)
- Hardcoded: Poster aspect, hover lift, badge styles

## MediaRow

- Source: `src/components/media/MediaRow.vue`
- Category: basic
- Description: Labeled horizontal row of MediaCards via BaseScroller.
- Extractable props: title (string), items (array)
- Hardcoded: Row title typography, spacing

## MediaPoster

- Source: `src/components/media/MediaPoster.vue`
- Category: basic
- Description: Poster image with fallback placeholder.
- Extractable props: src (string), title (string), aspect?
- Hardcoded: Object-fit cover, placeholder bg

## Admin domain patterns (candidates for redesign drafts)

## AdminMetricCards

- Source: `src/components/admin/AdminDashboard.vue`
- Category: basic
- Description: Dashboard metric card grid (jobs / scheduled / libraries).
- Extractable props: metrics (array of {title,value,meta,miniStats})
- Hardcoded: Grid layout, card chrome

## AdminSummaryGrid

- Source: `src/components/admin/AdminScheduledTasks.vue`
- Category: basic
- Description: Repeated summary-card row used by plugins/tasks pages.
- Extractable props: cards (array of {label,value,danger?})
- Hardcoded: summary-grid / summary-card class names

## AdminSettingsCard

- Source: `src/components/admin/AdminTranscode.vue`
- Category: basic
- Description: Settings section card with left brand indicator + form body.
- Extractable props: title (string), hint (string?)
- Hardcoded: indicator pill, settings-stack spacing

## AdminUserCard

- Source: `src/components/admin/AdminUsers.vue`
- Category: basic
- Description: User management card with avatar, role badge, facts, actions.
- Extractable props: user (object), disabled (boolean)
- Hardcoded: Emoji add button, role-badge colors, grid

## AdminLibPreviewCard

- Source: `src/components/admin/AdminLibraries.vue`
- Category: basic
- Description: Library preview card with accent bar, icon, path, count.
- Extractable props: lib (object), accent (color), iconPath (svg d)
- Hardcoded: Accent bar, path truncation, add-lib dashed card

## AdminSubTabs

- Source: `src/components/admin/AdminPlugins.vue`
- Category: basic
- Description: Underline/pill sub-tab bar for plugin panels.
- Extractable props: activeTab (string), tabs (array of {id,label})
- Hardcoded: sub-tabs-bar styles

## AdminTerminalLogs

- Source: `src/components/admin/AdminLogs.vue`
- Category: basic
- Description: Terminal-style log viewer with level filters and search.
- Extractable props: logs (array), currentLevelFilter (string), searchQuery (string)
- Hardcoded: Terminal chrome, level color classes, monospace
