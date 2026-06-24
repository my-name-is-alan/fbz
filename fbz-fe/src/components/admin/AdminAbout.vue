<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";

const uiStore = useUiStore();

const checking = ref(false);

function handleCheckUpdates() {
  checking.value = true;
  setTimeout(() => {
    checking.value = false;
    uiStore.showToast("您的自托管影视系统当前已是最新版本 (v0.1.0-alpha)！", "success");
  }, 1200);
}
</script>

<template>
  <div class="admin-about-view">
    <div class="about-card">
      <div class="brand">
        <span class="logo">F<b>B</b>Z</span>
        <span class="version">Server v0.1.0-alpha</span>
      </div>

      <div class="tech-stack-section">
        <h4>系统构建环境</h4>
        <div class="tech-grid">
          <div class="tech-item">
            <span class="tech-name">Vue</span>
            <span class="tech-val">v3.5.13</span>
          </div>
          <div class="tech-item">
            <span class="tech-name">Vite+ ( vp )</span>
            <span class="tech-val">v2.0.1</span>
          </div>
          <div class="tech-item">
            <span class="tech-name">Rolldown Bundler</span>
            <span class="tech-val">v1.0.0-beta</span>
          </div>
          <div class="tech-item">
            <span class="tech-name">Shaka Player</span>
            <span class="tech-val">v4.11.2</span>
          </div>
        </div>
      </div>

      <div class="about-info-text">
        <p>
          fbz
          是一款全功能、轻量化自托管家庭网络视频与媒体数据库管理系统。支持影片元数据智能搜刮过滤、多设备客户端串流播放、多版本视频文件加载及服务端硬件解码转码功能。
        </p>
        <p class="copyright">Copyright &copy; 2026 FBZ System. MIT Licensed.</p>
      </div>

      <footer class="about-footer">
        <button class="update-btn" type="button" :disabled="checking" @click="handleCheckUpdates">
          <span class="spinner" v-if="checking" />
          <span>{{ checking ? "正在检索更新..." : "检查系统更新" }}</span>
        </button>
      </footer>
    </div>
  </div>
</template>

<style scoped lang="scss">
.admin-about-view {
  max-width: 600px;
}

.about-card {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  padding: 32px;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.brand {
  display: flex;
  flex-direction: column;
  gap: 6px;
  align-items: center;
  text-align: center;

  .logo {
    font-family: var(--fbz-font-display);
    font-weight: 800;
    font-size: 36px;
    letter-spacing: 4px;
    color: var(--fbz-color-text);

    b {
      color: var(--fbz-color-brand-500);
    }
  }

  .version {
    font-size: var(--fbz-font-size-sm);
    color: var(--fbz-color-text-muted);
    font-weight: 700;
    letter-spacing: 1px;
  }
}

.tech-stack-section {
  border-top: 1px solid var(--fbz-color-line-soft);
  padding-top: 20px;

  h4 {
    margin: 0 0 12px;
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--fbz-color-text-muted);
    letter-spacing: 0.5px;
  }
}

.tech-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 12px;
}

.tech-item {
  display: flex;
  justify-content: space-between;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
  padding: 8px 14px;
  border-radius: 4px;
  font-size: var(--fbz-font-size-xs);

  .tech-name {
    font-weight: 700;
    color: var(--fbz-color-text-soft);
  }

  .tech-val {
    color: var(--fbz-color-text-muted);
    font-family: var(--fbz-font-display);
  }
}

.about-info-text {
  border-top: 1px solid var(--fbz-color-line-soft);
  padding-top: 20px;

  p {
    margin: 0 0 10px;
    font-size: 13px;
    color: var(--fbz-color-text-soft);
    line-height: 1.6;

    &:last-child {
      margin-bottom: 0;
    }
  }

  .copyright {
    font-size: 11px;
    color: var(--fbz-color-text-muted);
    margin-top: 14px;
  }
}

.about-footer {
  display: flex;
  justify-content: center;
  border-top: 1px solid var(--fbz-color-line-soft);
  padding-top: 20px;
}

.update-btn {
  height: 36px;
  padding: 0 var(--fbz-space-5);
  background: transparent;
  border: 1px solid var(--fbz-color-line-bright);
  color: var(--fbz-color-text-soft);
  font-weight: 700;
  font-size: var(--fbz-font-size-sm);
  border-radius: var(--fbz-radius-control);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  transition: all var(--fbz-motion-fast);

  &:hover:not(:disabled) {
    border-color: var(--fbz-color-brand-500);
    color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 3%, transparent);
  }

  &:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
}

.spinner {
  width: 12px;
  height: 12px;
  border: 2px solid var(--fbz-color-brand-500);
  border-top-color: transparent;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
