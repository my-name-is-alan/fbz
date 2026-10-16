<script setup lang="ts">
import type { CastMember } from "@/types/media.ts";

interface Props {
  cast: CastMember[];
  title?: string;
}

const props = withDefaults(defineProps<Props>(), {
  title: "演职人员",
});
</script>

<template>
  <section v-if="props.cast.length" class="cast-row">
    <h2 class="c-section-title">
      <BaseIcon name="users" :size="16" class="lead" />{{ props.title }}
      <span class="n">{{ props.cast.length }}</span>
    </h2>

    <BaseScroller class="scroller" col-width="var(--cast-col)" gap="var(--cast-gap)">
      <RouterLink
        v-for="member in props.cast"
        :key="member.linkId ?? member.id"
        :to="`/person/${encodeURIComponent(member.linkId ?? String(member.id))}`"
        class="cast-card"
      >
        <div class="avatar">
          <img
            v-if="member.profile_path"
            :src="member.profile_path"
            :alt="member.name"
            loading="lazy"
          />
          <BaseIcon v-else name="user-round" :size="30" :stroke-width="1.4" />
        </div>
        <p class="name" :title="member.name">{{ member.name }}</p>
        <p v-if="member.character" class="character" :title="member.character">
          饰 {{ member.character }}
        </p>
      </RouterLink>
    </BaseScroller>
  </section>
</template>

<style scoped lang="scss">
.cast-row {
  --cast-col: 92px;
  --cast-gap: 14px;

  display: flex;
  flex-direction: column;
  gap: 12px;

  .lead,
  .n {
    color: var(--ui-text-subtle);
  }

  .n {
    font-size: 12px;
    font-weight: 500;
  }
}

.scroller :deep(.track) {
  padding: 4px 2px 8px;
}

.cast-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  min-width: 0;
  color: inherit;
  text-align: center;
  text-decoration: none;

  &:hover {
    .avatar {
      border-color: var(--ui-accent);
    }

    .name {
      color: var(--ui-accent-hover);
    }
  }
}

.avatar {
  width: 80px;
  height: 80px;
  display: grid;
  place-items: center;
  overflow: hidden;
  border: 1px solid var(--ui-border-strong);
  border-radius: 50%;
  background: var(--ui-surface-raised);
  color: var(--ui-text-subtle);
  transition: border-color var(--ui-duration-fast);

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
}

.name,
.character {
  width: 100%;
  margin: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.name {
  margin-top: 8px;
  color: var(--ui-text);
  font-size: 12.5px;
  font-weight: 500;
  transition: color var(--ui-duration-fast);
}

.character {
  margin-top: 2px;
  color: var(--ui-text-muted);
  font-size: 11px;
}

@media (max-width: 600px) {
  .cast-row {
    --cast-col: 76px;
  }

  .avatar {
    width: 64px;
    height: 64px;
  }
}
</style>
