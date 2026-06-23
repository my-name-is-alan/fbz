<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";

const uiStore = useUiStore();
const { guidedTourActive } = storeToRefs(uiStore);

const currentStep = ref(1);

const steps = [
  {
    title: "1. 欢迎来到 FBZ 影视系统 🎬",
    text: "恭喜！系统初始化成功。这里是您的前台首页。现在所有的媒体卡片都可以进行流畅的点击、播放、甚至是右键高级控制。",
    targetSelector: ".sidebar-brand",
    positionHint: "底部",
  },
  {
    title: "2. 探索后台管理 ⚙️",
    text: "点击侧边栏的「后台管理」链接即可进入系统的管理后台。在管理后台中，您可以找到详细的「媒体库设置」，配置搜刮源、监控文件夹等深度功能。",
    targetSelector: ".menu-link[to='/admin']",
    positionHint: "右侧",
  },
  {
    title: "3. 右键动作与元数据编辑 🖱️",
    text: "主页或媒体网格中的每张影视海报均支持「右键单击」操作。右击可唤出上下文菜单，直接点击「编辑元数据」即可查询、更换海报，或修改基本简介信息。",
    targetSelector: ".media-card",
    positionHint: "上方",
  },
];

function nextStep() {
  if (currentStep.value < steps.length) {
    currentStep.value++;
  } else {
    finishTour();
  }
}

function prevStep() {
  if (currentStep.value > 1) {
    currentStep.value--;
  }
}

function finishTour() {
  uiStore.guidedTourActive = false;
  currentStep.value = 1;
}
</script>

<template>
  <Transition name="fade">
    <div v-if="guidedTourActive" class="tour-overlay" @click="finishTour">
      <div class="tour-card" @click.stop>
        <!-- Badge -->
        <span class="step-badge">新手引导: 步骤 {{ currentStep }} / {{ steps.length }}</span>

        <!-- Step content -->
        <div class="tour-content">
          <h3>{{ steps[currentStep - 1].title }}</h3>
          <p>{{ steps[currentStep - 1].text }}</p>
        </div>

        <!-- Spotlight Indicator Arrow -->
        <div class="tour-hint-box">
          <span class="spot-bullet" />
          <span class="spot-text"
            >提示：关注界面中的高亮区域 ({{ steps[currentStep - 1].positionHint }})</span
          >
        </div>

        <!-- Footer Actions -->
        <footer class="tour-footer">
          <button class="tour-btn text-btn" type="button" @click="finishTour">跳过</button>
          <div class="spacer" />
          <button v-if="currentStep > 1" class="tour-btn secondary" type="button" @click="prevStep">
            上一步
          </button>
          <button class="tour-btn primary" type="button" @click="nextStep">
            {{ currentStep === steps.length ? "我知道了" : "下一步" }}
          </button>
        </footer>
      </div>
    </div>
  </Transition>
</template>

<style scoped lang="scss">
.tour-overlay {
  position: fixed;
  inset: 0;
  z-index: 140;
  background: rgba(0, 0, 0, 0.45);
  display: flex;
  align-items: flex-end;
  justify-content: center;
  padding: 40px;
  pointer-events: auto;
}

.tour-card {
  width: 440px;
  max-width: 90vw;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-brand-500);
  border-radius: var(--fbz-radius-card);
  box-shadow:
    0 20px 50px rgba(0, 0, 0, 0.6),
    0 0 0 4px color-mix(in srgb, var(--fbz-color-brand-500) 15%, transparent);
  padding: var(--fbz-space-5);
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-3);
  position: relative;
  animation: slide-up 0.3s cubic-bezier(0.16, 1, 0.3, 1);
  color: var(--fbz-color-text);
  font-family: var(--fbz-font-sans);
}

@keyframes slide-up {
  from {
    transform: translateY(20px);
    opacity: 0;
  }
  to {
    transform: translateY(0);
    opacity: 1;
  }
}

.step-badge {
  align-self: flex-start;
  padding: 3px 8px;
  background: color-mix(in srgb, var(--fbz-color-brand-500) 8%, var(--fbz-color-panel-strong));
  border: 1px solid color-mix(in srgb, var(--fbz-color-brand-500) 25%, transparent);
  color: var(--fbz-color-brand-500);
  border-radius: var(--fbz-radius-round);
  font-size: var(--fbz-font-size-xs);
  font-weight: 700;
  letter-spacing: 0.5px;
}

.tour-content {
  h3 {
    margin: 0 0 8px;
    font-size: var(--fbz-font-size-md);
    font-weight: 800;
  }

  p {
    margin: 0;
    font-size: var(--fbz-font-size-sm);
    color: var(--fbz-color-text-soft);
    line-height: 1.6;
  }
}

.tour-hint-box {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px var(--fbz-space-3);
  border-radius: var(--fbz-radius-control);
  background: var(--fbz-color-bg-strong);
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-text-muted);

  .spot-bullet {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--fbz-color-brand-500);
    box-shadow: 0 0 8px var(--fbz-color-brand-500);
  }

  .spot-text {
    font-weight: 600;
  }
}

.tour-footer {
  display: flex;
  align-items: center;
  margin-top: 4px;

  .spacer {
    flex: 1;
  }

  .tour-btn {
    height: 34px;
    padding: 0 14px;
    border-radius: var(--fbz-radius-control);
    font-size: var(--fbz-font-size-sm);
    font-weight: 700;
    cursor: pointer;
    transition: all var(--fbz-motion-fast);

    &.text-btn {
      background: transparent;
      border: 0;
      color: var(--fbz-color-text-muted);
      font-weight: 600;

      &:hover {
        color: var(--fbz-color-text);
      }
    }

    &.secondary {
      background: var(--fbz-color-panel-strong);
      border: 1px solid var(--fbz-color-line);
      color: var(--fbz-color-text-soft);
      margin-right: 6px;

      &:hover {
        background: var(--fbz-color-panel-elevated);
        color: var(--fbz-color-text);
      }
    }

    &.primary {
      background: var(--fbz-color-brand-500);
      border: 0;
      color: #07120a;

      &:hover {
        background: var(--fbz-color-brand-600);
      }
    }
  }
}

// Fade transitions
.fade-enter-active,
.fade-leave-active {
  transition: opacity var(--fbz-motion-base) ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
