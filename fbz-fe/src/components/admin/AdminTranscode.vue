<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";

const uiStore = useUiStore();

// Form states
const hwAcceleration = ref("none");
const maxResolution = ref("auto");
const audioChannels = ref("6");
const enableH265 = ref(true);
const burnSubtitles = ref("picture");
const saving = ref(false);

const hwOptions = [
  { label: "禁用硬件加速 (CPU 软件解码)", value: "none" },
  { label: "Intel QuickSync (QSV)", value: "qsv" },
  { label: "NVIDIA NVENC / NVDEC", value: "nvenc" },
  { label: "AMD AMF Video Engine", value: "amf" },
  { label: "VAAPI (Linux 通用)", value: "vaapi" },
];

const resolutionOptions = [
  { label: "不限分辨率 (自动匹配网络带宽)", value: "auto" },
  { label: "最大限制为 4K (2160P)", value: "4k" },
  { label: "最大限制为 1080P (Full HD)", value: "1080p" },
  { label: "最大限制为 720P (Standard HD)", value: "720p" },
];

const channelOptions = [
  { label: "支持 5.1 / 7.1 声道环绕立体声", value: "6" },
  { label: "强制降混为 双声道立体声 (Stereo)", value: "2" },
];

const subtitleOptions = [
  { label: "仅烧录图形字幕 (PGS, VOBSUB)", value: "picture" },
  { label: "总是烧录所有格式字幕", value: "all" },
  { label: "从不烧录字幕 (使用客户端软解渲染)", value: "never" },
];

function handleSave() {
  saving.value = true;
  setTimeout(() => {
    saving.value = false;
    uiStore.showToast("媒体串流与硬件转码参数配置已成功应用！", "success");
  }, 1000);
}
</script>

<template>
  <div class="admin-transcode-view">
    <div class="settings-stack">
      <!-- Section 1: HW Transcoding -->
      <section class="settings-card">
        <div class="card-header">
          <span class="indicator" />
          <h3>硬件加速编解码</h3>
        </div>
        <div class="card-body">
          <div class="form-group">
            <label for="transcode-hw">硬件加速类型</label>
            <BaseSelect
              id="transcode-hw"
              v-model="hwAcceleration"
              :options="hwOptions"
              ariaLabel="选择硬件加速解码器"
            />
            <span class="field-hint"
              >选用对应的 GPU 解码器可大幅减小服务器转码时的 CPU 消耗。如果是 Docker
              容器部署，请确认已穿透对应的 GPU 设备节点。</span
            >
          </div>

          <div class="toggle-list">
            <div class="toggle-row">
              <div class="toggle-info">
                <span class="title">启用 H.265 / HEVC 硬件编码</span>
                <span class="desc"
                  >对兼容的浏览器和客户端自动采用高压缩率的 H.265 编码输出，减少 50%
                  传输带宽。</span
                >
              </div>
              <label class="glow-switch" aria-label="启用 H.265 编码">
                <input type="checkbox" v-model="enableH265" />
                <span class="switch-slide-thumb" />
              </label>
            </div>
          </div>
        </div>
      </section>

      <!-- Section 2: Quality & Video Stream -->
      <section class="settings-card">
        <div class="card-header">
          <span class="indicator" />
          <h3>串流带宽与画质限制</h3>
        </div>
        <div class="card-body">
          <div class="form-group">
            <label for="transcode-quality">最大转码分辨率</label>
            <BaseSelect
              id="transcode-quality"
              v-model="maxResolution"
              :options="resolutionOptions"
              ariaLabel="选择最大画质限制"
            />
          </div>
        </div>
      </section>

      <!-- Section 3: Subtitles & Audio -->
      <section class="settings-card">
        <div class="card-header">
          <span class="indicator" />
          <h3>音频与字幕烧录</h3>
        </div>
        <div class="card-body">
          <div class="form-group">
            <label for="transcode-audio">音频输出声道</label>
            <BaseSelect
              id="transcode-audio"
              v-model="audioChannels"
              :options="channelOptions"
              ariaLabel="选择音频声道"
            />
          </div>

          <div class="form-group">
            <label for="transcode-subtitles">字幕烧录策略</label>
            <BaseSelect
              id="transcode-subtitles"
              v-model="burnSubtitles"
              :options="subtitleOptions"
              ariaLabel="选择字幕烧录策略"
            />
            <span class="field-hint"
              >部分客户端（如 Chrome 浏览器）无法原生渲染 PGS
              蓝光图形字幕，此时必须通过服务器进行服务端烧录转码输出。</span
            >
          </div>
        </div>
      </section>

      <!-- Actions Footer -->
      <footer class="actions-footer">
        <button class="btn-primary" type="button" :disabled="saving" @click="handleSave">
          <span class="spinner" v-if="saving" />
          <span>{{ saving ? "正在保存..." : "保存转码配置" }}</span>
        </button>
      </footer>
    </div>
  </div>
</template>

<style scoped lang="scss">
.admin-transcode-view {
  max-width: 800px;
}

.settings-stack {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.toggle-list {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
  margin-top: var(--fbz-space-2);
}

.toggle-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--fbz-space-6);

  .toggle-info {
    display: flex;
    flex-direction: column;
    gap: 3px;

    .title {
      font-size: var(--fbz-font-size-sm);
      font-weight: 700;
      color: var(--fbz-color-text);
    }

    .desc {
      font-size: var(--fbz-font-size-xs);
      color: var(--fbz-color-text-muted);
      line-height: 1.4;
    }
  }
}

.glow-switch {
  position: relative;
  display: inline-block;
  width: 44px;
  height: 22px;
  flex-shrink: 0;

  input {
    opacity: 0;
    width: 0;
    height: 0;
  }

  .switch-slide-thumb {
    position: absolute;
    cursor: pointer;
    inset: 0;
    background-color: var(--fbz-color-line-bright);
    border-radius: 22px;
    transition: background-color var(--fbz-motion-fast);

    &::before {
      position: absolute;
      content: "";
      height: 16px;
      width: 16px;
      left: 3px;
      bottom: 3px;
      background-color: white;
      border-radius: 50%;
      transition: transform var(--fbz-motion-fast);
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
    }
  }

  input:checked + .switch-slide-thumb {
    background-color: var(--fbz-color-brand-500);

    &::before {
      transform: translateX(22px);
    }
  }

  input:focus-visible + .switch-slide-thumb {
    box-shadow: var(--fbz-shadow-focus);
  }
}

.field-hint {
  font-size: 11px;
  color: var(--fbz-color-text-muted);
  line-height: 1.4;
}

.actions-footer {
  display: flex;
  justify-content: flex-start;
  padding-top: var(--fbz-space-2);
}

.btn-primary {
  height: 38px;
  padding: 0 var(--fbz-space-6);
  background: var(--fbz-color-brand-500);
  border: 0;
  color: #07120a;
  font-weight: 700;
  font-size: var(--fbz-font-size-sm);
  border-radius: var(--fbz-radius-control);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  transition: all var(--fbz-motion-fast);

  &:hover:not(:disabled) {
    background: var(--fbz-color-brand-600);
  }

  &:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
}

.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid #07120a;
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
