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

    <BaseScroller class="scroller" col-width="var(--cast-col)" gap="var(--cast-gap)">
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
          <div v-else class="avatar-ph">
            <svg class="silhouette-svg" viewBox="0 0 24 24" fill="currentColor">
              <path
                d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"
              />
            </svg>
            <span class="initial">{{ member.name.charAt(0) }}</span>
          </div>
        </div>
        <p class="name" :title="member.name">{{ member.name }}</p>
        <p class="character" :title="member.character">{{ member.character }}</p>
      </RouterLink>
    </BaseScroller>
  </section>
</template>

<style scoped lang="scss">
.cast-row {
  --cast-col: 96px;
  --cast-gap: var(--fbz-space-4);

  max-width: 1280px;
  margin: 0 auto;
  padding: 0 var(--fbz-space-8) var(--fbz-space-8);
}

.section-title {
  margin: 0 0 var(--fbz-space-4);
  font-size: 18px;
  font-weight: 800;
  letter-spacing: -0.2px;
}

.cast-card {
  text-decoration: none;
  color: inherit;
  display: flex;
  flex-direction: column;
  align-items: center;
}

.avatar {
  position: relative;
  width: var(--cast-col);
  height: var(--cast-col);
  aspect-ratio: 1;
  border-radius: 50%;
  overflow: hidden;
  border: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel);
  box-shadow: 0 4px 10px rgba(0, 0, 0, 0.2);
  transition:
    border-color var(--fbz-motion-base),
    box-shadow var(--fbz-motion-base),
    transform var(--fbz-motion-base);

  .cast-card:hover & {
    border-color: var(--fbz-color-brand-500);
    box-shadow:
      0 8px 18px color-mix(in srgb, var(--fbz-color-brand-500) 20%, transparent),
      0 4px 8px rgba(0, 0, 0, 0.3);
    transform: scale(1.06) translateY(-2px);
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
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  background: var(--fbz-color-panel-strong);
  color: var(--fbz-color-text-disabled);

  .silhouette-svg {
    width: 45%;
    height: 45%;
    opacity: 0.7;
    margin-bottom: 2px;
  }

  .initial {
    font-size: var(--fbz-font-size-xs);
    font-weight: 800;
    color: var(--fbz-color-text-muted);
    font-family: var(--fbz-font-display);
    text-transform: uppercase;
  }
}

.name {
  margin: 10px 0 3px;
  font-size: 12px;
  font-weight: 700;
  line-height: 1.3;
  text-align: center;
  width: 100%;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  color: var(--fbz-color-text);
  transition: color var(--fbz-motion-fast);

  .cast-card:hover & {
    color: var(--fbz-color-brand-500);
  }
}

.character {
  margin: 0;
  font-size: 11px;
  line-height: 1.25;
  color: var(--fbz-color-text-muted);
  text-align: center;
  width: 100%;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

@media (max-width: 600px) {
  .cast-row {
    --cast-col: 80px;
    --cast-gap: 12px;

    padding: 0 var(--fbz-space-4) var(--fbz-space-5);
  }

  .name {
    font-size: 11px;
    margin-top: 8px;
  }

  .character {
    font-size: 10px;
  }
}
</style>
