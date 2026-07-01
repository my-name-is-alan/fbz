<script setup lang="ts">
import { userAvatarUrl } from "@/service/request.ts";

/**
 * 通用用户头像：优先展示 `/api/users/{id}/avatar` 真实图，加载失败（未设置头像→404）
 * 回退为昵称首字母 + 稳定派生色。顶部 header、个人资料、用户管理列表共用同一个。
 */
interface Props {
  userId?: string | null;
  /** 用于首字母与派生底色的名称 */
  name?: string;
  /** 像素直径 */
  size?: number;
  /** 头像缓存版本（改动后传新值可击穿浏览器缓存） */
  version?: string | number | null;
}

const props = withDefaults(defineProps<Props>(), {
  userId: null,
  name: "",
  size: 34,
  version: null,
});

// 加载失败标记：同一 src 变化时重置，允许换头像后重新尝试。
const failed = ref(false);

const src = computed(() => userAvatarUrl(props.userId, props.version));
watch(src, () => {
  failed.value = false;
});

const initial = computed(() => (props.name || "?").trim().charAt(0).toUpperCase() || "?");

// 由名称派生稳定的 HSL 底色（同名同色），保证无图时也有辨识度。
const fallbackColor = computed(() => {
  const source = props.name || props.userId || "?";
  let hash = 0;
  for (let i = 0; i < source.length; i += 1) {
    hash = (hash * 31 + source.charCodeAt(i)) % 360;
  }
  return `hsl(${hash}, 42%, 42%)`;
});

const showImage = computed(() => !!src.value && !failed.value);
</script>

<template>
  <span
    class="base-avatar"
    :style="{
      width: `${size}px`,
      height: `${size}px`,
      fontSize: `${Math.round(size * 0.42)}px`,
      background: showImage ? 'transparent' : fallbackColor,
    }"
    :title="name || undefined"
  >
    <img v-if="showImage" :src="src" :alt="name || '用户头像'" @error="failed = true" />
    <span v-else aria-hidden="true">{{ initial }}</span>
  </span>
</template>

<style scoped lang="scss">
.base-avatar {
  display: inline-grid;
  place-content: center;
  border-radius: 50%;
  overflow: hidden;
  flex: 0 0 auto;
  color: #ffffff;
  font-weight: 700;
  line-height: 1;
  user-select: none;

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
}
</style>
