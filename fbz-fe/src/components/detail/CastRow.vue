<script setup lang="ts">
import type { CastMember } from "@/types/media.ts";
import { imageUrl } from "@/service/modules/tmdb.ts";

interface Props {
  cast: CastMember[];
}

const props = defineProps<Props>();
</script>

<template>
  <section v-if="props.cast.length" class="cast-row">
    <h2 class="section-title">演职人员</h2>

    <BaseScroller class="scroller" col-width="var(--cast-col)" gap="var(--fbz-space-2)">
      <RouterLink
        v-for="member in props.cast"
        :key="member.id"
        :to="`/person/${member.id}`"
        class="cast-card"
      >
        <div class="avatar">
          <img
            v-if="imageUrl(member.profile_path, 'w200')"
            :src="imageUrl(member.profile_path, 'w200')"
            :alt="member.name"
            loading="lazy"
          />
          <span v-else class="avatar-ph">{{ member.name.charAt(0) }}</span>
        </div>
        <p class="name">{{ member.name }}</p>
        <p class="character">{{ member.character }}</p>
      </RouterLink>
    </BaseScroller>
  </section>
</template>

<style scoped lang="scss">
.cast-row {
  --cast-col: 64px;

  max-width: 1280px;
  margin: 0 auto;
  padding: 0 var(--fbz-space-8) var(--fbz-space-8);
}

.section-title {
  margin: 0 0 var(--fbz-space-4);
  font-size: 16px;
  font-weight: 700;
}

.cast-card {
  text-decoration: none;
  color: inherit;
}

.avatar {
  position: relative;
  aspect-ratio: 1;
  border-radius: 50%;
  overflow: hidden;
  border: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel);
  transition:
    border-color var(--fbz-motion-fast),
    transform var(--fbz-motion-fast);

  .cast-card:hover & {
    border-color: var(--fbz-color-brand-500);
    transform: translateY(-2px);
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
}

.avatar-ph {
  position: absolute;
  inset: 0;
  display: grid;
  place-content: center;
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  color: var(--fbz-color-text-muted);
}

.name {
  margin: 7px 0 2px;
  font-size: 11px;
  font-weight: 600;
  line-height: 1.25;
  text-align: center;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.character {
  margin: 0;
  font-size: 10px;
  line-height: 1.25;
  color: var(--fbz-color-text-muted);
  text-align: center;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

@media (max-width: 600px) {
  .cast-row {
    --cast-col: 56px;

    padding: 0 var(--fbz-space-4) var(--fbz-space-5);
  }

  .name {
    font-size: 11px;
  }

  .character {
    font-size: 10px;
  }
}
</style>
