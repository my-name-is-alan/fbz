<script setup lang="ts">
import { getTranscodeSettings, updateTranscodeSettings } from "@/service/modules/admin.ts";
import { useUiStore } from "@/stores/ui.ts";
import type { TranscodeSettings } from "@/types/admin.ts";

const uiStore = useUiStore();

// 表单状态，初值对齐后端 allowlist，加载后被真实设置覆盖。
const hardwareAcceleration = ref("none");
const preferredEncoder = ref("h264");
const maxResolution = ref("original");
const segmentDuration = ref(6);
const throttle = ref(true);

const loading = ref(false);
const saving = ref(false);
const loadError = ref("");

// hardwareAcceleration ∈ none/nvenc/qsv/vaapi/videotoolbox（后端 allowlist）
const hwOptions = [
  { label: "禁用硬件加速 (CPU 软件解码)", value: "none" },
  { label: "NVIDIA NVENC / NVDEC", value: "nvenc" },
  { label: "Intel QuickSync (QSV)", value: "qsv" },
  { label: "VAAPI (Linux 通用)", value: "vaapi" },
  { label: "VideoToolbox (macOS)", value: "videotoolbox" },
];

const encoderOptions = [
  { label: "H.264 / AVC (兼容性最佳)", value: "h264" },
  { label: "H.265 / HEVC (高压缩率)", value: "h265" },
  { label: "AV1 (下一代高效编码)", value: "av1" },
];

// maxResolution ∈ 480/720/1080/2160/original（后端 allowlist）
const resolutionOptions = [
  { label: "不限分辨率 (原始质量)", value: "original" },
  { label: "最大限制为 4K (2160P)", value: "2160" },
  { label: "最大限制为 1080P (Full HD)", value: "1080" },
  { label: "最大限制为 720P (Standard HD)", value: "720" },
  { label: "最大限制为 480P (SD)", value: "480" },
];

const segmentOptions = [
  { label: "2 秒 (低延迟，切片更多)", value: 2 },
  { label: "4 秒", value: 4 },
  { label: "6 秒 (推荐)", value: 6 },
  { label: "10 秒 (更少切片请求)", value: 10 },
];

onMounted(() => {
  void loadSettings();
});

async function loadSettings() {
  loading.value = true;
  loadError.value = "";
  try {
    const settings = await getTranscodeSettings();
    applySettings(settings);
  } catch {
    loadError.value = "转码设置加载失败，请确认后端已就绪且当前账号具备管理员权限。";
  } finally {
    loading.value = false;
  }
}

function applySettings(settings: TranscodeSettings) {
  hardwareAcceleration.value = settings.hardwareAcceleration;
  preferredEncoder.value = settings.preferredEncoder;
  maxResolution.value = settings.maxResolution;
  segmentDuration.value = settings.segmentDuration;
  throttle.value = settings.throttle;
}

async function handleSave() {
  saving.value = true;
  try {
    const saved = await updateTranscodeSettings({
      hardwareAcceleration: hardwareAcceleration.value,
      preferredEncoder: preferredEncoder.value,
      maxResolution: maxResolution.value,
      segmentDuration: segmentDuration.value,
      throttle: throttle.value,
    });
    applySettings(saved);
    uiStore.showToast("媒体串流与硬件转码参数配置已成功保存。", "success");
  } catch {
    uiStore.showToast("保存转码配置失败，请检查参数或后端状态。", "error");
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div class="admin-transcode-view">
    <p v-if="loadError" class="load-error">{{ loadError }}</p>
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
              v-model="hardwareAcceleration"
              :options="hwOptions"
              ariaLabel="选择硬件加速解码器"
            />
            <span class="field-hint"
              >选用对应的 GPU 解码器可大幅减小服务器转码时的 CPU 消耗。如果是 Docker
              容器部署，请确认已穿透对应的 GPU 设备节点。</span
            >
          </div>

          <div class="form-group">
            <label for="transcode-encoder">首选视频编码器</label>
            <BaseSelect
              id="transcode-encoder"
              v-model="preferredEncoder"
              :options="encoderOptions"
              ariaLabel="选择首选视频编码器"
            />
            <span class="field-hint"
              >对兼容的客户端优先采用所选编码输出，H.265 / AV1 可显著减少传输带宽。</span
            >
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

          <div class="form-group">
            <label for="transcode-segment">HLS 切片时长</label>
            <BaseSelect
              id="transcode-segment"
              v-model="segmentDuration"
              :options="segmentOptions"
              ariaLabel="选择 HLS 切片时长"
            />
          </div>

          <div class="toggle-list">
            <div class="toggle-row">
              <div class="toggle-info">
                <span class="title">启用转码限速 (Throttle)</span>
                <span class="desc"
                  >当客户端缓冲充足时暂缓转码，降低服务器峰值负载与磁盘写入压力。</span
                >
              </div>
              <label class="glow-switch" aria-label="启用转码限速">
                <input type="checkbox" v-model="throttle" />
                <span class="switch-slide-thumb" />
              </label>
            </div>
          </div>
        </div>
      </section>

      <!-- Actions Footer -->
      <footer class="actions-footer">
        <button class="btn-primary" type="button" :disabled="saving || loading" @click="handleSave">
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

.load-error {
  margin: 0 0 var(--fbz-space-4);
  color: var(--fbz-color-danger-500);
  font-size: var(--fbz-font-size-sm);
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
