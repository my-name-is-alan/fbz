<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";
import { useThemeStore } from "@/stores/theme.ts";
import { useLibraryStore } from "@/stores/library.ts";

const uiStore = useUiStore();
const themeStore = useThemeStore();
const libraryStore = useLibraryStore();

const currentStep = ref(1);

// Step 1: Agreement
const agreed = ref(false);

// Step 2: Admin Account
const username = ref("admin");
const password = ref("");
const confirmPassword = ref("");
const passwordStrength = computed(() => {
  const pwd = password.value;
  if (!pwd) return { label: "空", class: "empty", width: "0%" };
  if (pwd.length < 6) return { label: "弱", class: "weak", width: "30%" };

  const hasLetters = /[a-zA-Z]/.test(pwd);
  const hasNumbers = /[0-9]/.test(pwd);
  const hasSpecial = /[^a-zA-Z0-9]/.test(pwd);

  if (pwd.length >= 8 && hasLetters && hasNumbers && hasSpecial) {
    return { label: "强", class: "strong", width: "100%" };
  }
  if (hasLetters && hasNumbers) {
    return { label: "中", class: "medium", width: "65%" };
  }
  return { label: "弱", class: "weak", width: "30%" };
});

const passwordError = computed(() => {
  if (confirmPassword.value && password.value !== confirmPassword.value) {
    return "密码与确认密码不一致";
  }
  return "";
});

// Step 3: Add library (optional)
const skipLibrary = ref(false);
const libraryTitle = ref("我的电影");
const libraryType = ref("movie");
const libraryPath = ref("/media/nas/电影");

const libraryTypeOptions = [
  { label: "电影", value: "movie" },
  { label: "电视剧", value: "series" },
  { label: "动漫", value: "anime" },
  { label: "纪录片", value: "documentary" },
  { label: "音乐", value: "music" },
];

async function browsePath() {
  try {
    const selectedPath = await uiStore.openFilePicker();
    if (selectedPath) {
      libraryPath.value = selectedPath;
    }
  } catch (err) {
    console.error(err);
  }
}

// Step 4: Theme Preferences
const presetColors = [
  { label: "经典绿", value: "#1ed760" },
  { label: "爱奇艺红", value: "#e50914" },
  { label: "天空蓝", value: "#0063e5" },
  { label: "芒果黄", value: "#ff9900" },
  { label: "优雅紫", value: "#8b5cf6" },
  { label: "科技青", value: "#00f5d4" },
];

// Navigation
function nextStep() {
  if (currentStep.value === 1 && !agreed.value) return;
  if (currentStep.value === 2 && (passwordError.value || !password.value || !username.value))
    return;

  if (currentStep.value < 4) {
    currentStep.value++;
  } else {
    // Finish initialization
    // Add library to store if not skipped
    if (!skipLibrary.value && libraryTitle.value && libraryPath.value) {
      libraryStore.libraries.push({
        id: `custom-${Date.now()}`,
        name: libraryTitle.value,
        kind: libraryType.value as any,
        count: 12, // mock some item count
      });
    }
    uiStore.completeInitialization();
  }
}

function prevStep() {
  if (currentStep.value > 1) {
    currentStep.value--;
  }
}
</script>

<template>
  <Transition name="fade">
    <div v-if="uiStore.setupWizardOpen" class="wizard-overlay">
      <div class="wizard-container">
        <!-- Sidebar Navigation Tracker -->
        <aside class="wizard-steps-aside">
          <div class="brand">
            <span class="logo">F<b>B</b>Z</span>
            <span class="version">初始化向导 v1.0</span>
          </div>

          <div class="steps-tracker">
            <div class="tracker-item" :class="{ active: currentStep === 1, done: currentStep > 1 }">
              <span class="idx">1</span>
              <span class="label">许可协议</span>
            </div>
            <div class="tracker-item" :class="{ active: currentStep === 2, done: currentStep > 2 }">
              <span class="idx">2</span>
              <span class="label">创建管理员</span>
            </div>
            <div class="tracker-item" :class="{ active: currentStep === 3, done: currentStep > 3 }">
              <span class="idx">3</span>
              <span class="label">新建媒体库</span>
            </div>
            <div class="tracker-item" :class="{ active: currentStep === 4, done: currentStep > 4 }">
              <span class="idx">4</span>
              <span class="label">偏好设置</span>
            </div>
          </div>

          <div class="aside-footer">自托管影视媒体库管理系统</div>
        </aside>

        <!-- Main Form Body -->
        <section class="wizard-form-body">
          <!-- Step 1: Agreement -->
          <div v-if="currentStep === 1" class="wizard-step step-1">
            <h1>欢迎使用 fbz 影视系统 🎬</h1>
            <p class="subtitle">
              fbz 是一款全能型自托管影视媒体库管理系统。在您开始探索之前，我们需要达成以下使用协议：
            </p>

            <div class="agreement-box">
              <h3>使用许可及免责声明</h3>
              <p>
                1.
                本系统主要用于整合、整理及播放您的本地家庭媒体收藏。用户应保证其所导入和播放的媒体内容具有合法使用权或所有权。
              </p>
              <p>
                2. 系统自动抓取的网络元数据（包括来自 TMDB, IMDb
                等机构的数据）仅用作参考，元数据之版权归原作者及服务提供商所有。
              </p>
              <p>
                3.
                软件作者不对由于系统搭建、端口暴露、第三方非法注入所引发的数据损毁、隐私泄露或侵权行为承担任何直接及间接法律责任。
              </p>
              <p>
                4.
                严禁使用本系统进行公开商业放映或非授权的大范围网络点播分发，一切违规操作责任自负。
              </p>
            </div>

            <label class="agreement-check">
              <input v-model="agreed" type="checkbox" />
              <span class="check-box-display" />
              <span class="check-text">我已阅读并完全同意上述许可与免责协议</span>
            </label>
          </div>

          <!-- Step 2: Create Admin Account -->
          <div v-if="currentStep === 2" class="wizard-step step-2">
            <h1>创建管理员账号 👤</h1>
            <p class="subtitle">
              管理员账号拥有控制台最高权限，用于管理系统任务、搜刮引擎和全局设置。
            </p>

            <div class="form-group">
              <label>管理员用户名</label>
              <input
                v-model="username"
                type="text"
                placeholder="输入管理员名称"
                class="control-input"
              />
            </div>

            <div class="form-group">
              <label>登录密码</label>
              <input
                v-model="password"
                type="password"
                placeholder="请输入高强度密码"
                class="control-input"
              />

              <!-- Password strength meter -->
              <div v-if="password" class="strength-meter-container">
                <span class="strength-label"
                  >强度:
                  <b :class="passwordStrength.class">{{ passwordStrength.label }}</b>
                </span>
                <div class="strength-track">
                  <div
                    class="strength-fill"
                    :class="passwordStrength.class"
                    :style="{ width: passwordStrength.width }"
                  />
                </div>
              </div>
            </div>

            <div class="form-group">
              <label>确认登录密码</label>
              <input
                v-model="confirmPassword"
                type="password"
                placeholder="请再次输入密码"
                class="control-input"
                :class="{ 'is-invalid': passwordError }"
              />
              <span v-if="passwordError" class="error-text">{{ passwordError }}</span>
            </div>
          </div>

          <!-- Step 3: Create Media Library -->
          <div v-if="currentStep === 3" class="wizard-step step-3">
            <div class="step-header">
              <h1>添加首个媒体库 📁</h1>
              <label class="skip-toggle">
                <input v-model="skipLibrary" type="checkbox" />
                <span>稍后在控制台中添加</span>
              </label>
            </div>
            <p class="subtitle">
              媒体库可以将服务器路径下的视频文件关联至本系统。配置搜刮器后将自动呈现精美的海报墙。
            </p>

            <div v-if="!skipLibrary" class="library-setup-fields">
              <div class="form-group">
                <label>媒体库类型</label>
                <BaseSelect v-model="libraryType" :options="libraryTypeOptions" class="w-full" />
              </div>

              <div class="form-group">
                <label>媒体库标题</label>
                <input
                  v-model="libraryTitle"
                  type="text"
                  placeholder="输入媒体库的显示标题，如：我的电影"
                  class="control-input"
                />
              </div>

              <div class="form-group">
                <label>媒体夹路径 (绝对路径)</label>
                <div class="input-with-button">
                  <input
                    v-model="libraryPath"
                    type="text"
                    placeholder="例如：/media/nas/电影"
                    class="control-input"
                  />
                  <button class="browse-btn" type="button" @click="browsePath">📁 浏览</button>
                </div>
                <span class="hint">系统将监控此目录，发现新影片后自动执行元数据匹配搜刮。</span>
              </div>
            </div>
            <div v-else class="skip-tip">
              <span class="skip-icon">💡</span>
              <p>
                已勾选跳过。初始化完成后，您可以在系统的“后台管理”->“媒体库设置”中随时导入和扫描媒体目录。
              </p>
            </div>
          </div>

          <!-- Step 4: Theme Preferences -->
          <div v-if="currentStep === 4" class="wizard-step step-4">
            <h1>主题与偏好设置 🎨</h1>
            <p class="subtitle">定制属于您的视觉风格。此项偏好后期可随时在“个人设置”中更改。</p>

            <div class="settings-group">
              <label class="group-title">系统视觉主题</label>
              <div class="theme-options">
                <button
                  class="theme-opt-card"
                  :class="{ active: themeStore.themeMode === 'dark' }"
                  type="button"
                  @click="themeStore.setThemeMode('dark')"
                >
                  <div class="opt-preview dark-preview">
                    <span class="preview-dot" />
                    <span class="preview-line" />
                  </div>
                  <span class="opt-label">暗黑模式</span>
                </button>
                <button
                  class="theme-opt-card"
                  :class="{ active: themeStore.themeMode === 'light' }"
                  type="button"
                  @click="themeStore.setThemeMode('light')"
                >
                  <div class="opt-preview light-preview">
                    <span class="preview-dot" />
                    <span class="preview-line" />
                  </div>
                  <span class="opt-label">明亮模式</span>
                </button>
              </div>
            </div>

            <div class="settings-group color-group">
              <label class="group-title">品牌主题色</label>
              <div class="color-options">
                <button
                  v-for="color in presetColors"
                  :key="color.value"
                  class="color-dot-btn"
                  :class="{ active: themeStore.brandColor === color.value }"
                  :style="{ '--dot-color': color.value }"
                  type="button"
                  :title="color.label"
                  @click="themeStore.setBrandColor(color.value)"
                >
                  <span class="check-mark">✓</span>
                </button>

                <div class="custom-color-picker">
                  <label class="picker-label">
                    <input
                      type="color"
                      :value="themeStore.brandColor"
                      @input="(e) => themeStore.setBrandColor((e.target as HTMLInputElement).value)"
                      class="color-input"
                    />
                    <span
                      class="picker-display-dot"
                      :style="{ background: themeStore.brandColor }"
                    />
                    <span class="picker-text">自定义</span>
                  </label>
                </div>
              </div>
            </div>
          </div>

          <!-- Modal Footer Actions -->
          <footer class="wizard-footer">
            <button
              v-if="currentStep > 1"
              class="wizard-btn secondary"
              type="button"
              @click="prevStep"
            >
              上一步
            </button>
            <div class="spacer" />
            <button
              class="wizard-btn primary"
              :disabled="
                (currentStep === 1 && !agreed) ||
                (currentStep === 2 && (passwordError || !password || !username))
              "
              type="button"
              @click="nextStep"
            >
              {{ currentStep === 4 ? "完成初始化" : "下一步" }}
            </button>
          </footer>
        </section>
      </div>
    </div>
  </Transition>
</template>

<style scoped lang="scss">
.wizard-overlay {
  position: fixed;
  inset: 0;
  z-index: 120;
  background: var(--fbz-color-bg);
  display: grid;
  place-content: center;
  overflow-y: auto;
  padding: 40px var(--fbz-space-4);
}

.wizard-container {
  width: 900px;
  max-width: 95vw;
  height: 600px;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
  border-radius: var(--fbz-radius-card);
  box-shadow: var(--fbz-shadow-panel);
  display: flex;
  overflow: hidden;
}

.wizard-steps-aside {
  width: 250px;
  border-right: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-bg-strong);
  padding: var(--fbz-space-6);
  display: flex;
  flex-direction: column;
  justify-content: space-between;

  .brand {
    display: flex;
    flex-direction: column;
    gap: 4px;

    .logo {
      font-family: var(--fbz-font-display);
      font-weight: 800;
      font-size: 24px;
      letter-spacing: 3px;
      color: var(--fbz-color-text);

      b {
        color: var(--fbz-color-brand-500);
      }
    }

    .version {
      font-size: 11px;
      color: var(--fbz-color-text-muted);
      letter-spacing: 1px;
    }
  }

  .steps-tracker {
    display: flex;
    flex-direction: column;
    gap: var(--fbz-space-5);
    margin: 40px 0;
  }

  .tracker-item {
    display: flex;
    align-items: center;
    gap: 12px;
    opacity: 0.4;
    transition: opacity var(--fbz-motion-base);

    .idx {
      width: 28px;
      height: 28px;
      border: 1px solid var(--fbz-color-line);
      border-radius: 50%;
      display: grid;
      place-content: center;
      font-size: var(--fbz-font-size-sm);
      font-weight: 700;
      color: var(--fbz-color-text-soft);
      background: var(--fbz-color-panel);
      transition: all var(--fbz-motion-base);
    }

    .label {
      font-size: var(--fbz-font-size-sm);
      font-weight: 700;
      color: var(--fbz-color-text-soft);
    }

    &.active {
      opacity: 1;

      .idx {
        border-color: var(--fbz-color-brand-500);
        color: var(--fbz-color-brand-500);
        box-shadow: var(--fbz-shadow-focus);
      }
    }

    &.done {
      opacity: 0.85;

      .idx {
        border-color: var(--fbz-color-brand-500);
        background: var(--fbz-color-brand-500);
        color: #07120a;
      }
    }
  }

  .aside-footer {
    font-size: 11px;
    color: var(--fbz-color-text-disabled);
  }
}

.wizard-form-body {
  flex: 1;
  padding: var(--fbz-space-6) var(--fbz-space-8);
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  overflow-y: auto;
}

.wizard-step {
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: center;

  h1 {
    margin: 0 0 var(--fbz-space-2);
    font-size: var(--fbz-font-size-xl);
    font-weight: 900;
  }

  .subtitle {
    margin: 0 0 var(--fbz-space-5);
    font-size: var(--fbz-font-size-md);
    color: var(--fbz-color-text-soft);
    line-height: 1.6;
  }
}

.agreement-box {
  flex: 1;
  max-height: 200px;
  overflow-y: auto;
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-control);
  background: var(--fbz-color-bg-strong);
  padding: var(--fbz-space-4);
  margin-bottom: var(--fbz-space-4);
  font-size: var(--fbz-font-size-sm);
  color: var(--fbz-color-text-soft);
  line-height: 1.6;

  h3 {
    margin: 0 0 var(--fbz-space-2);
    font-size: 13px;
    font-weight: 800;
    color: var(--fbz-color-text);
  }

  p {
    margin: 0 0 var(--fbz-space-2);

    &:last-child {
      margin-bottom: 0;
    }
  }
}

.agreement-check {
  display: flex;
  align-items: center;
  gap: 10px;
  cursor: pointer;
  user-select: none;
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  color: var(--fbz-color-text);

  input {
    display: none;
  }

  .check-box-display {
    width: 18px;
    height: 18px;
    border: 1px solid var(--fbz-color-line);
    border-radius: 4px;
    background: var(--fbz-color-panel-strong);
    display: grid;
    place-content: center;
    transition: all var(--fbz-motion-fast);

    &::after {
      content: "✓";
      color: #07120a;
      font-size: 11px;
      font-weight: 900;
      opacity: 0;
    }
  }

  input:checked + .check-box-display {
    border-color: var(--fbz-color-brand-500);
    background: var(--fbz-color-brand-500);

    &::after {
      opacity: 1;
    }
  }
}

.form-group {
  margin-bottom: var(--fbz-space-4);
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);

  label {
    font-size: var(--fbz-font-size-sm);
    font-weight: 700;
    color: var(--fbz-color-text);
  }

  .control-input {
    height: 40px;
    background: var(--fbz-color-bg-strong);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    padding: 0 var(--fbz-space-3);
    color: var(--fbz-color-text);
    font-size: var(--fbz-font-size-sm);

    &:focus {
      outline: none;
      border-color: var(--fbz-color-brand-500);
    }

    &.is-invalid {
      border-color: var(--fbz-color-danger-500);
    }
  }

  .error-text {
    font-size: var(--fbz-font-size-xs);
    color: var(--fbz-color-danger-500);
    font-weight: 600;
  }

  .hint {
    font-size: var(--fbz-font-size-xs);
    color: var(--fbz-color-text-muted);
  }
}

.strength-meter-container {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 4px;

  .strength-label {
    font-size: 11px;
    color: var(--fbz-color-text-muted);

    b {
      &.weak {
        color: var(--fbz-color-danger-500);
      }
      &.medium {
        color: var(--fbz-color-amber-500);
      }
      &.strong {
        color: var(--fbz-color-brand-500);
      }
    }
  }

  .strength-track {
    height: 4px;
    background: var(--fbz-color-line);
    border-radius: 2px;
    overflow: hidden;
  }

  .strength-fill {
    height: 100%;
    transition:
      width 0.3s ease,
      background 0.3s ease;

    &.weak {
      background: var(--fbz-color-danger-500);
    }
    &.medium {
      background: var(--fbz-color-amber-500);
    }
    &.strong {
      background: var(--fbz-color-brand-500);
    }
  }
}

.step-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: var(--fbz-space-2);

  h1 {
    margin: 0;
  }

  .skip-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    font-size: var(--fbz-font-size-sm);
    font-weight: 700;
    color: var(--fbz-color-brand-500);
  }
}

.input-with-button {
  display: flex;
  gap: var(--fbz-space-2);

  input {
    flex: 1;
  }

  .browse-btn {
    height: 40px;
    padding: 0 var(--fbz-space-4);
    background: var(--fbz-color-panel-strong);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    color: var(--fbz-color-text-soft);
    font-weight: 700;
    cursor: pointer;

    &:hover {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
    }
  }
}

.skip-tip {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 40px var(--fbz-space-6);
  border: 1px dashed var(--fbz-color-line);
  border-radius: var(--fbz-radius-control);
  background: var(--fbz-color-bg-strong);
  color: var(--fbz-color-text-soft);
  text-align: center;

  .skip-icon {
    font-size: 32px;
    margin-bottom: var(--fbz-space-3);
  }

  p {
    margin: 0;
    max-width: 420px;
    font-size: var(--fbz-font-size-sm);
    line-height: 1.6;
  }
}

.settings-group {
  margin-bottom: var(--fbz-space-5);

  .group-title {
    display: block;
    margin-bottom: var(--fbz-space-3);
    font-size: var(--fbz-font-size-sm);
    font-weight: 800;
    color: var(--fbz-color-text);
  }
}

.theme-options {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--fbz-space-4);
}

.theme-opt-card {
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel-strong);
  border-radius: var(--fbz-radius-card);
  padding: var(--fbz-space-3);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);
  align-items: center;
  transition: all var(--fbz-motion-base);

  &:hover {
    border-color: var(--fbz-color-line-bright);
    transform: translateY(-2px);
  }

  &.active {
    border-color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 4%, var(--fbz-color-panel-strong));
    box-shadow: 0 4px 12px color-mix(in srgb, var(--fbz-color-brand-500) 8%, transparent);
  }
}

.opt-preview {
  width: 100%;
  height: 50px;
  border-radius: var(--fbz-radius-control);
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--fbz-space-2);
  border: 1px solid var(--fbz-color-line-soft);
}

.dark-preview {
  background: #0a0a0b;
  .preview-dot {
    background: #1ed760;
  }
  .preview-line {
    background: #ffffff;
  }
}

.light-preview {
  background: #f5f5f7;
  .preview-dot {
    background: #0063e5;
  }
  .preview-line {
    background: #1c1c1e;
  }
}

.preview-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}

.preview-line {
  width: 48px;
  height: 6px;
  border-radius: 3px;
  opacity: 0.8;
}

.opt-label {
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  color: var(--fbz-color-text-soft);
}

.theme-opt-card.active .opt-label {
  color: var(--fbz-color-brand-500);
}

.color-options {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--fbz-space-3);
}

.color-dot-btn {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  border: 2px solid var(--fbz-color-line);
  background: var(--dot-color);
  cursor: pointer;
  position: relative;
  display: grid;
  place-content: center;
  box-shadow: 0 4px 10px rgba(0, 0, 0, 0.15);
  transition: all var(--fbz-motion-fast);

  &:hover {
    transform: scale(1.15);
  }

  .check-mark {
    color: #fff;
    font-size: 11px;
    font-weight: 900;
    opacity: 0;
  }

  &.active {
    border-color: var(--fbz-color-text);
    transform: scale(1.08);

    .check-mark {
      opacity: 1;
    }
  }
}

.custom-color-picker {
  margin-left: 4px;
}

.picker-label {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-2);
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

  &:hover {
    border-color: var(--fbz-color-line-bright);
    background: var(--fbz-color-panel-elevated);
  }
}

.color-input {
  position: absolute;
  top: 0;
  left: 0;
  opacity: 0;
  width: 100%;
  height: 100%;
  cursor: pointer;
}

.picker-display-dot {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  border: 1px solid var(--fbz-color-line);
}

.picker-text {
  line-height: 1;
}

.wizard-footer {
  border-top: 1px solid var(--fbz-color-line-soft);
  padding-top: var(--fbz-space-4);
  display: flex;
  align-items: center;

  .spacer {
    flex: 1;
  }

  .wizard-btn {
    height: 40px;
    padding: 0 var(--fbz-space-6);
    border-radius: var(--fbz-radius-control);
    font-size: var(--fbz-font-size-sm);
    font-weight: 700;
    cursor: pointer;
    transition: all var(--fbz-motion-fast);

    &.secondary {
      background: var(--fbz-color-panel-strong);
      border: 1px solid var(--fbz-color-line);
      color: var(--fbz-color-text-soft);

      &:hover {
        background: var(--fbz-color-panel-elevated);
        color: var(--fbz-color-text);
      }
    }

    &.primary {
      background: var(--fbz-color-brand-500);
      border: 0;
      color: #07120a;

      &:hover:not(:disabled) {
        background: var(--fbz-color-brand-600);
      }

      &:disabled {
        opacity: 0.5;
        cursor: not-allowed;
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
