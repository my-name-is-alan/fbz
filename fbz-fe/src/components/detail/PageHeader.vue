<script setup lang="ts">
interface Crumb {
  label: string;
  to?: string;
  icon?: string;
}

interface Props {
  title?: string;
  fallback?: string;
  /** 层级面包屑（剧集 → 季 → 集 / 媒体库 → 条目）。 */
  crumbs?: Crumb[];
}

const props = withDefaults(defineProps<Props>(), {
  fallback: "/",
  crumbs: () => [],
});

const router = useRouter();

function goBack() {
  if (window.history.state?.back) {
    router.back();
    return;
  }
  void router.push(props.fallback);
}
</script>

<template>
  <div class="page-header">
    <button class="back-btn" type="button" aria-label="返回上一页" @click="goBack">
      <BaseIcon name="back" :size="16" />
      <span>返回</span>
    </button>
    <nav v-if="props.crumbs.length || props.title" class="crumbs" aria-label="面包屑">
      <template v-for="(crumb, i) in props.crumbs" :key="`${crumb.label}-${i}`">
        <RouterLink v-if="crumb.to" :to="crumb.to" class="crumb">
          <BaseIcon v-if="crumb.icon" :name="crumb.icon" :size="13" />{{ crumb.label }}
        </RouterLink>
        <span v-else class="crumb">{{ crumb.label }}</span>
        <BaseIcon name="right" :size="12" class="sep" />
      </template>
      <span v-if="props.title" class="crumb current">{{ props.title }}</span>
    </nav>
  </div>
</template>

<style scoped lang="scss">
.page-header {
  position: relative;
  z-index: 2;
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}

.back-btn {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 30px;
  padding: 0 10px 0 8px;
  border: 1px solid var(--ui-glass-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-glass);
  color: var(--ui-text);
  font-size: 12.5px;
  backdrop-filter: blur(10px);
  cursor: pointer;

  &:hover {
    border-color: var(--ui-border-strong);
    background: var(--ui-surface-hover);
  }
}

.crumbs {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  overflow: hidden;
  color: var(--ui-text-subtle);
  font-size: 12.5px;
  white-space: nowrap;
}

.crumb {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  overflow: hidden;
  color: var(--ui-text-muted);
  text-decoration: none;
  text-overflow: ellipsis;

  &:is(a):hover {
    color: var(--ui-accent);
  }

  &.current {
    color: var(--ui-text);
  }
}

.sep {
  flex: 0 0 auto;
  opacity: 0.6;
}
</style>
