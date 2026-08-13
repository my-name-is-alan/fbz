<script setup lang="ts">
/**
 * 主题设置：暗/亮模式切换、品牌主色选择、初始化向导重置。
 */
import { useThemeStore } from "@/stores/theme.ts";
import { useUiStore } from "@/stores/ui.ts";

const themeStore = useThemeStore();
const uiStore = useUiStore();

// 预设品牌色选项
const presetColors = [
  { label: "经典绿", value: "#1ed760" },
  { label: "爱奇艺红", value: "#e50914" },
  { label: "天空蓝", value: "#0063e5" },
  { label: "芒果黄", value: "#ff9900" },
  { label: "优雅紫", value: "#8b5cf6" },
  { label: "科技青", value: "#00f5d4" },
];
</script>

<template>
  <div class="personalization-section">
    <div class="style-settings-stack">
      <!-- Card 1: Theme selection -->
      <section class="settings-card">
        <div class="card-header">
          <span class="indicator" />
          <h3>系统主题外观</h3>
        </div>
        <div class="card-body">
          <p class="settings-hint">选择您偏好的视觉背景模式。</p>
          <div class="theme-options-grid">
            <button
              class="theme-card dark-opt"
              :class="{ active: themeStore.themeMode === 'dark' }"
              type="button"
              @click="themeStore.setThemeMode('dark')"
            >
              <div class="theme-preview dark-preview">
                <span class="circle-dot" />
                <span class="line-bar" />
              </div>
              <span class="label">暗黑模式 (Dark Mode)</span>
            </button>

            <button
              class="theme-card light-opt"
              :class="{ active: themeStore.themeMode === 'light' }"
              type="button"
              @click="themeStore.setThemeMode('light')"
            >
              <div class="theme-preview light-preview">
                <span class="circle-dot" />
                <span class="line-bar" />
              </div>
              <span class="label">明亮模式 (Light Mode)</span>
            </button>
          </div>
        </div>
      </section>

      <!-- Card 2: Brand Color selection -->
      <section class="settings-card">
        <div class="card-header">
          <span class="indicator" />
          <h3>全局强调主色调</h3>
        </div>
        <div class="card-body">
          <p class="settings-hint">更改主操作按钮、图标、激活状态和播放进度条的色系。</p>
          <div class="color-options-flex">
            <button
              v-for="color in presetColors"
              :key="color.value"
              class="brand-color-dot"
              :class="{ active: themeStore.brandColor === color.value }"
              :style="{ '--color-val': color.value }"
              type="button"
              :title="color.label"
              @click="themeStore.setBrandColor(color.value)"
            >
              <svg
                v-if="themeStore.brandColor === color.value"
                viewBox="0 0 24 24"
                width="12"
                height="12"
                fill="none"
                stroke="#fff"
                stroke-width="3"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <polyline points="20 6 9 17 4 12" />
              </svg>
            </button>

            <div class="custom-color-picker-wrapper">
              <label class="custom-picker-btn">
                <input
                  type="color"
                  :value="themeStore.brandColor"
                  @input="(e) => themeStore.setBrandColor((e.target as HTMLInputElement).value)"
                  class="hidden-color-input"
                />
                <span
                  class="color-indicator-circle"
                  :style="{ background: themeStore.brandColor }"
                />
                <span class="text">自定义色彩</span>
              </label>
            </div>
          </div>
        </div>
      </section>

      <!-- Card 3: Reset / Dev tools -->
      <section class="settings-card dev-card">
        <div class="card-header">
          <span class="indicator dev-indicator" />
          <h3>开发调试与重置</h3>
        </div>
        <div class="card-body">
          <p class="settings-hint">您可以清空本地缓存，重新激活首次进入向导流程以测试配置效果。</p>
          <button class="relaunch-wizard-btn" type="button" @click="uiStore.resetInitialization">
            <svg
              viewBox="0 0 24 24"
              width="14"
              height="14"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <polyline points="23 4 23 10 17 10" />
              <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
            </svg>
            重新拉起初始化向导
          </button>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped lang="scss">
.personalization-section {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-5);
}

.style-settings-stack {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.settings-card {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-card);
  overflow: hidden;

  .card-header {
    padding: var(--fbz-space-3) var(--fbz-space-5);
    border-bottom: 1px solid var(--fbz-color-line-soft);
    display: flex;
    align-items: center;
    gap: 10px;

    .indicator {
      width: 3px;
      height: 12px;
      background: var(--fbz-color-brand-500);
      border-radius: 2px;

      &.dev-indicator {
        background: var(--fbz-color-amber-500);
      }
    }

    h3 {
      margin: 0;
      font-size: 12px;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      color: var(--fbz-color-text-soft);
    }
  }

  .card-body {
    padding: var(--fbz-space-5);
    display: flex;
    flex-direction: column;
    gap: var(--fbz-space-4);
  }
}

.settings-hint {
  margin: 0 0 var(--fbz-space-3);
  font-size: var(--fbz-font-size-sm);
  color: var(--fbz-color-text-muted);
}

.theme-options-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--fbz-space-4);
}

.theme-card {
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel-strong);
  border-radius: var(--fbz-radius-card);
  padding: var(--fbz-space-4);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-3);
  align-items: center;
  transition: all var(--fbz-motion-base);

  &:hover {
    border-color: var(--fbz-color-line-bright);
    transform: translateY(-1px);
  }

  &.active {
    border-color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 4%, var(--fbz-color-panel-strong));

    .label {
      color: var(--fbz-color-brand-500);
    }
  }

  .theme-preview {
    width: 100%;
    height: 56px;
    border-radius: var(--fbz-radius-control);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--fbz-space-2);
    border: 1px solid var(--fbz-color-line-soft);
  }

  .dark-preview {
    background: #0a0a0b;
    .circle-dot {
      background: #1ed760;
    }
    .line-bar {
      background: #ffffff;
    }
  }

  .light-preview {
    background: #f5f5f7;
    .circle-dot {
      background: #0063e5;
    }
    .line-bar {
      background: #1c1c1e;
    }
  }

  .circle-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .line-bar {
    width: 48px;
    height: 5px;
    border-radius: 3px;
    opacity: 0.8;
  }

  .label {
    font-size: var(--fbz-font-size-sm);
    font-weight: 700;
    color: var(--fbz-color-text-soft);
  }
}

.color-options-flex {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--fbz-space-3);
}

.brand-color-dot {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  border: 2px solid var(--fbz-color-line);
  background: var(--color-val);
  cursor: pointer;
  position: relative;
  display: grid;
  place-content: center;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
  transition: all var(--fbz-motion-fast) cubic-bezier(0.175, 0.885, 0.32, 1.275);

  &:hover {
    transform: scale(1.12);
  }

  &.active {
    border-color: var(--fbz-color-text);
    transform: scale(1.08);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-val) 35%, transparent);
  }
}

.custom-color-picker-wrapper {
  margin-left: 4px;
}

.custom-picker-btn {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 32px;
  padding: 0 var(--fbz-space-3);
  border-radius: var(--fbz-radius-round);
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel-strong);
  font-size: var(--fbz-font-size-xs);
  font-weight: 700;
  color: var(--fbz-color-text-soft);
  cursor: pointer;
  position: relative;
  overflow: hidden;
  transition: all var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-line-bright);
    background: var(--fbz-color-panel-elevated);
  }

  .hidden-color-input {
    position: absolute;
    top: 0;
    left: 0;
    opacity: 0;
    width: 100%;
    height: 100%;
    cursor: pointer;
  }

  .color-indicator-circle {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 1px solid var(--fbz-color-line);
  }
}

.relaunch-wizard-btn {
  height: 36px;
  padding: 0 var(--fbz-space-4);
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line);
  color: var(--fbz-color-text-soft);
  border-radius: var(--fbz-radius-control);
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  transition: all var(--fbz-motion-fast);

  svg {
    flex-shrink: 0;
  }

  &:hover {
    border-color: var(--fbz-color-brand-500);
    color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 3%, transparent);
  }
}

@media (max-width: 768px) {
  .theme-options-grid {
    grid-template-columns: 1fr;
  }
}
</style>
