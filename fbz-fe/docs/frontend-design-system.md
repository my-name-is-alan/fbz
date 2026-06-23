# AetherNAS Frontend Design System

## Visual Direction

AetherNAS uses a dark home-theater control surface: low-light panels, crisp borders, cool active states, and compact information density. Components should feel operational rather than promotional.

## Token Rules

- Use `src/styles/theme/tokens.scss` as the source of truth for colors, spacing, radius, typography, shadow, focus, and motion.
- Expose runtime values as `--fbz-*` CSS variables through `fbz-theme-vars`.
- Keep `uno.config.ts` aligned with token names when a token is useful in atomic classes.
- Do not hardcode new colors in page components unless they represent one-off artwork or imported media.

## Component Layers

- `src/components/base`: generic UI primitives. They know about visual states, slots, accessibility, and sizing, but not media-library domain data.
- `src/components/media`: AetherNAS domain components. They can accept `MediaCardItem`, `EpisodeItem`, `MetricItem`, or media-specific props.
- `src/views`: route composition only. Route views choose data, layout, and navigation; they should not duplicate reusable component internals.

## Base Component Naming

- Use the `Base` prefix for primitives: `BaseButton`, `BaseInput`, `BaseSelect`, `BaseTextarea`, `BaseSwitch`, `BaseSegmentedControl`, `BaseFilterBar`, `BaseTable`, `BaseModal`, `BaseTag`, `BasePanel`, `BaseProgress`, `BaseEmpty`, `BaseIconButton`.
- Use PascalCase filenames and component names.
- Use typed props and emits.
- Prefer slots for optional icons, actions, and rich content.

## Media Components

- Use `MediaCard` for poster/continue-watching cards.
- Use `MediaInfoCard` when poster artwork, tags, and definition-list metadata must travel together.
- Use `MediaTagGroup` for all media capability/status tags instead of hand-placing `BaseTag` lists.
- Use `MediaPlayerControls` for playback bars, keeping progress as a `v-model:progress` contract and actions as explicit emits.
- Keep page-level media views as composition surfaces; do not duplicate card, metadata, tag, or player markup in route files.

## State Semantics

- `brand`: primary product action.
- `info`: neutral technical or playback state.
- `success`: healthy/available/completed state.
- `warning`: attention-needed or queued state.
- `danger`: destructive or failed state.
- `gold`: premium media capability, rating, or quality badge.

## Styling Rules

- SFC styles must use `<style scoped lang="scss">`.
- Prefer token variables over raw values.
- Use class selectors in scoped styles.
- Use UnoCSS for layout utilities when local component styles do not need to encode reusable behavior.
- Keep card radius at `8px` unless the component is an application shell or hero surface.

## Accessibility Rules

- Icon-only controls require an `aria-label`.
- Inputs need a stable `id` and `name`.
- Progress indicators expose `role="progressbar"` and `aria-valuenow`.
- Avoid using color alone to communicate status; pair color with text labels.
