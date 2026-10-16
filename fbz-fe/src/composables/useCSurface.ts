import { onBeforeUnmount, onMounted } from "vue";

/** C 端视觉作用域 class；样式见 `styles/theme/hdhive.scss`。 */
export const C_SURFACE_CLASS = "fbz-c";

let holders = 0;

/**
 * 在 `<html>` 上挂 C 端作用域（HDHive token）。后台布局不调用，Admin 视觉保持冻结。
 * 用引用计数而非布尔：路由在两个 C 端壳之间切换时不会出现一帧无作用域。
 */
export function useCSurface(): void {
  onMounted(() => {
    holders += 1;
    document.documentElement.classList.add(C_SURFACE_CLASS);
  });
  onBeforeUnmount(() => {
    holders = Math.max(0, holders - 1);
    if (!holders) document.documentElement.classList.remove(C_SURFACE_CLASS);
  });
}
