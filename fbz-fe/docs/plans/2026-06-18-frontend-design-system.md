# Frontend Design System Implementation Plan

> **For Claude:** Use `${SUPERPOWERS_SKILLS_ROOT}/skills/collaboration/executing-plans/SKILL.md` to implement this plan task-by-task.

**Goal:** Build the first AetherNAS frontend design-system layer: tokens, base components, media-specific examples, and a living `/system` showcase.

**Architecture:** Keep design primitives in `src/components/base`, domain-specific media components in `src/components/media`, and shared visual values in `src/styles/theme/tokens.scss`. Route views remain composition surfaces and reuse components instead of carrying local duplicated UI styles.

**Tech Stack:** Vue 3, TypeScript, Composition API, SCSS, UnoCSS, Vite+.

---

### Task 1: Expand Theme Tokens

**Files:**

- Modify: `src/styles/theme/tokens.scss`
- Modify: `uno.config.ts`

**Steps:**

1. Add semantic colors for surfaces, text, borders, action, status, focus, and danger states.
2. Add spacing, type, radius, shadow, z-index, and motion variables.
3. Mirror commonly used semantic tokens into UnoCSS theme keys.
4. Run `vp check --fix`.

### Task 2: Create Base Components

**Files:**

- Modify: `src/components/base/BaseButton.vue`
- Create: `src/components/base/BaseIconButton.vue`
- Create: `src/components/base/BaseInput.vue`
- Create: `src/components/base/BaseTag.vue`
- Create: `src/components/base/BasePanel.vue`
- Create: `src/components/base/BaseEmpty.vue`
- Create: `src/components/base/BaseProgress.vue`

**Steps:**

1. Keep each base component presentation-focused with typed props and slots.
2. Use `<script setup lang="ts">` and `<style scoped lang="scss">`.
3. Do not bind base components to media-specific data structures.
4. Run `vp check --fix`.

### Task 3: Create Media Examples From Base Components

**Files:**

- Create: `src/components/media/EpisodeRow.vue`
- Create: `src/components/media/InfoList.vue`
- Create: `src/components/media/MetricTile.vue`
- Modify: `src/views/admin/index.vue`
- Modify: `src/views/media/detail/index.vue`

**Steps:**

1. Extract repeated detail/admin rows into typed media components.
2. Keep route pages responsible only for layout and data selection.
3. Reuse `BasePanel`, `BaseProgress`, and base token variables.
4. Run `vp check --fix`.

### Task 4: Build Living Design-System Page

**Files:**

- Modify: `src/views/system/index.vue`
- Create: `docs/frontend-design-system.md`

**Steps:**

1. Replace the static system page with sections for tokens, typography, controls, feedback, surfaces, and media examples.
2. Document component naming, token usage, SCSS/UnoCSS boundaries, and state semantics.
3. Run `vp check --fix`, `vp test`, and `vp run build`.
4. Verify `/system`, `/`, and `/media/detail` in browser.
