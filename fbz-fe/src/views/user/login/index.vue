<script setup lang="ts">
import { useAuthStore } from "@/stores/auth.ts";
import { useUiStore } from "@/stores/ui.ts";

const router = useRouter();
const route = useRoute();
const authStore = useAuthStore();
const uiStore = useUiStore();

const columns = [0, 1, 2, 3];
const tilesPerColumn = 8;

/* ---------- 表单状态 ---------- */
const username = ref(authStore.username || "");
const password = ref("");
const remember = ref(localStorage.getItem("fbz_authenticated") === "true");
const showPassword = ref(false);
const language = ref(authStore.language);
const loading = ref(false);

const languageOptions = [
  { label: "简体中文", value: "zh-CN" },
  { label: "繁體中文", value: "zh-TW" },
  { label: "English", value: "en-US" },
  { label: "日本語", value: "ja-JP" },
];

const features = ["Emby 兼容", "硬件转码", "插件生态", "多用户权限"];

const appVersion = "FBZ Server · v0.1.0";

async function handleLogin() {
  if (loading.value) return;
  loading.value = true;

  const ok = await authStore.login({
    username: username.value,
    password: password.value,
    remember: remember.value,
  });

  loading.value = false;
  if (!ok) return;

  authStore.setLanguage(language.value);
  const redirect = route.query.redirect;
  await router.push(typeof redirect === "string" ? redirect : "/");
}

function handleForgot() {
  // 对应后端 Users/ForgotPassword 的 ContactAdmin 行为
  uiStore.showToast("请联系服务器管理员重置您的登录密码。", "info");
}
</script>

<template>
  <div class="login-view">
    <!-- 左：媒体海报墙 + 品牌 -->
    <section class="login-art" aria-hidden="true">
      <div class="poster-wall">
        <div
          v-for="(col, ci) in columns"
          :key="ci"
          class="poster-col"
          :class="ci % 2 === 0 ? 'drift-up' : 'drift-down'"
        >
          <div v-for="tile in tilesPerColumn" :key="`${col}-${tile}`" class="poster-tile">
            <span class="poster-fallback">{{ (col * tilesPerColumn + tile) % 9 }}</span>
          </div>
        </div>
      </div>

      <div class="art-veil" />

      <div class="art-brand">
        <div class="brand-mark">
          <span class="brand-glyph">◢</span>
          <span class="brand-word">FBZ</span>
        </div>
        <h1 class="art-title">自托管媒体中心</h1>
        <p class="art-sub">扫描、刮削、转码与串流，统一管理你的本地影视与音乐媒体库。</p>
        <ul class="art-features">
          <li v-for="f in features" :key="f"><span class="dot" />{{ f }}</li>
        </ul>
      </div>
    </section>

    <!-- 右：登录面板 -->
    <section class="login-panel">
      <div class="panel-inner">
        <!-- 移动端品牌 -->
        <div class="mobile-brand">
          <span class="brand-glyph">◢</span>
          <span class="brand-word">FBZ</span>
        </div>

        <header class="panel-head">
          <span class="indicator" />
          <div>
            <h2>登录到 FBZ</h2>
            <p>输入账户凭据以进入媒体工作台。</p>
          </div>
        </header>

        <form class="login-form" @submit.prevent="handleLogin">
          <div class="form-group">
            <label for="login-username">用户名</label>
            <div class="control-wrap">
              <svg class="lead-icon" viewBox="0 0 24 24" aria-hidden="true">
                <path
                  d="M12 12a4 4 0 100-8 4 4 0 000 8zM5 20a7 7 0 0114 0"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.6"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>
              <input
                id="login-username"
                v-model="username"
                type="text"
                placeholder="请输入登录用户名"
                class="control-input has-icon"
                autocomplete="username"
              />
            </div>
          </div>

          <div class="form-group">
            <label for="login-password">登录密码</label>
            <div class="control-wrap">
              <svg class="lead-icon" viewBox="0 0 24 24" aria-hidden="true">
                <path
                  d="M6 10V8a6 6 0 0112 0v2m-13 0h14v10H5z"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.6"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>
              <input
                id="login-password"
                v-model="password"
                :type="showPassword ? 'text' : 'password'"
                placeholder="请输入密码"
                class="control-input has-icon has-trail"
                autocomplete="current-password"
              />
              <button
                type="button"
                class="trail-btn"
                :aria-label="showPassword ? '隐藏密码' : '显示密码'"
                @click="showPassword = !showPassword"
              >
                <svg v-if="showPassword" viewBox="0 0 24 24" aria-hidden="true">
                  <path
                    d="M3 3l18 18M10.6 10.6a2 2 0 002.8 2.8M9.9 5.1A9.8 9.8 0 0112 5c5 0 9 4.5 10 7-0.4 1-1.3 2.3-2.6 3.5M6.1 6.1C4 7.4 2.6 9.2 2 12c1 2.5 5 7 10 7 1.2 0 2.3-.2 3.3-.6"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.6"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  />
                </svg>
                <svg v-else viewBox="0 0 24 24" aria-hidden="true">
                  <path
                    d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7-10-7-10-7z"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.6"
                    stroke-linejoin="round"
                  />
                  <circle
                    cx="12"
                    cy="12"
                    r="3"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.6"
                  />
                </svg>
              </button>
            </div>
          </div>

          <div class="form-row">
            <label class="remember">
              <span class="glow-switch">
                <input v-model="remember" type="checkbox" />
                <span class="switch-slide-thumb" />
              </span>
              <span class="remember-text">记住此设备</span>
            </label>
            <button type="button" class="link-btn" @click="handleForgot">忘记密码？</button>
          </div>

          <button type="submit" class="submit-btn" :class="{ loading }" :disabled="loading">
            <span v-if="loading" class="spinner" />
            <span>{{ loading ? "正在登录…" : "登 录" }}</span>
          </button>
        </form>

        <footer class="panel-foot">
          <div class="lang-group">
            <span class="foot-label">界面语言</span>
            <BaseSelect
              v-model="language"
              :options="languageOptions"
              size="sm"
              aria-label="界面语言"
              class="lang-select"
            />
          </div>
          <span class="version">{{ appVersion }}</span>
        </footer>
      </div>
    </section>
  </div>
</template>

<style scoped lang="scss">
.login-view {
  display: grid;
  grid-template-columns: 1.05fr 0.95fr;
  min-height: 100vh;
  background: var(--fbz-color-bg);
  color: var(--fbz-color-text);

  @media (max-width: 900px) {
    grid-template-columns: 1fr;
  }
}

/* ---------------- 左侧海报墙 ---------------- */
.login-art {
  position: relative;
  overflow: hidden;
  background: var(--fbz-color-bg-strong);
  border-right: 1px solid var(--fbz-color-line-soft);

  @media (max-width: 900px) {
    display: none;
  }
}

.poster-wall {
  position: absolute;
  inset: -8% -4%;
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: var(--fbz-space-3);
  transform: rotate(-6deg) scale(1.18);
  opacity: 0.55;
}

.poster-col {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-3);

  &.drift-up {
    animation: drift-up 38s linear infinite;
  }
  &.drift-down {
    animation: drift-down 44s linear infinite;
  }
}

.poster-tile {
  aspect-ratio: 2 / 3;
  border-radius: var(--fbz-radius-card);
  overflow: hidden;
  background: linear-gradient(
    135deg,
    color-mix(in srgb, var(--fbz-color-brand-500) 20%, var(--fbz-color-panel-strong)),
    var(--fbz-color-panel)
  );
  border: 1px solid var(--fbz-color-line-soft);

  .poster-fallback {
    width: 100%;
    height: 100%;
    display: grid;
    place-content: center;
    font-family: var(--fbz-font-display);
    font-size: 28px;
    color: var(--fbz-color-text-muted);
  }
}

// 漂移：移动一个 tile 高度，循环无缝
@keyframes drift-up {
  from {
    transform: translateY(0);
  }
  to {
    transform: translateY(-12%);
  }
}
@keyframes drift-down {
  from {
    transform: translateY(-12%);
  }
  to {
    transform: translateY(0);
  }
}

.art-veil {
  position: absolute;
  inset: 0;
  background:
    radial-gradient(
      120% 90% at 18% 88%,
      color-mix(in srgb, var(--fbz-color-brand-500) 22%, transparent) 0%,
      transparent 52%
    ),
    linear-gradient(
      75deg,
      var(--fbz-color-bg-strong) 12%,
      color-mix(in srgb, var(--fbz-color-bg-strong) 70%, transparent) 48%,
      color-mix(in srgb, var(--fbz-color-bg-strong) 30%, transparent) 100%
    );
}

.art-brand {
  position: absolute;
  left: var(--fbz-space-8);
  bottom: var(--fbz-space-8);
  right: var(--fbz-space-8);
  z-index: 1;
}

.brand-mark,
.mobile-brand {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  font-family: var(--fbz-font-display);
  font-weight: 800;

  .brand-glyph {
    color: var(--fbz-color-brand-500);
    font-size: 26px;
    line-height: 1;
    filter: drop-shadow(0 0 10px color-mix(in srgb, var(--fbz-color-brand-500) 55%, transparent));
  }

  .brand-word {
    font-size: 30px;
    letter-spacing: 4px;
    color: var(--fbz-color-text);
  }
}

.art-title {
  margin: var(--fbz-space-5) 0 var(--fbz-space-2);
  font-size: 30px;
  font-weight: 800;
  letter-spacing: 1px;
}

.art-sub {
  margin: 0;
  max-width: 420px;
  font-size: var(--fbz-font-size-md);
  line-height: 1.6;
  color: var(--fbz-color-text-soft);
}

.art-features {
  list-style: none;
  margin: var(--fbz-space-5) 0 0;
  padding: 0;
  display: flex;
  flex-wrap: wrap;
  gap: var(--fbz-space-2) var(--fbz-space-3);

  li {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 5px 12px 5px 9px;
    border-radius: var(--fbz-radius-round);
    border: 1px solid var(--fbz-color-line);
    background: color-mix(in srgb, var(--fbz-color-panel) 70%, transparent);
    -webkit-backdrop-filter: blur(8px);
    backdrop-filter: blur(8px);
    font-size: var(--fbz-font-size-xs);
    font-weight: 600;
    color: var(--fbz-color-text-soft);
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--fbz-color-brand-500);
    box-shadow: 0 0 6px color-mix(in srgb, var(--fbz-color-brand-500) 70%, transparent);
  }
}

/* ---------------- 右侧登录面板 ---------------- */
.login-panel {
  display: grid;
  place-items: center;
  padding: var(--fbz-space-8) var(--fbz-space-6);
  background:
    radial-gradient(
      90% 60% at 80% 0%,
      color-mix(in srgb, var(--fbz-color-brand-500) 7%, transparent),
      transparent 60%
    ),
    var(--fbz-color-bg);
}

.panel-inner {
  width: 100%;
  max-width: 380px;
}

.mobile-brand {
  display: none;
  margin-bottom: var(--fbz-space-6);

  @media (max-width: 900px) {
    display: inline-flex;
  }

  .brand-glyph {
    font-size: 22px;
  }
  .brand-word {
    font-size: 26px;
  }
}

.panel-head {
  display: flex;
  align-items: flex-start;
  gap: var(--fbz-space-3);
  margin-bottom: var(--fbz-space-6);

  .indicator {
    flex: 0 0 auto;
    width: 3px;
    height: 38px;
    border-radius: 2px;
    background: var(--fbz-color-brand-500);
    box-shadow: 0 0 10px color-mix(in srgb, var(--fbz-color-brand-500) 45%, transparent);
  }

  h2 {
    margin: 0 0 4px;
    font-size: var(--fbz-font-size-xl);
    font-weight: 800;
    letter-spacing: 0.5px;
  }

  p {
    margin: 0;
    font-size: var(--fbz-font-size-sm);
    color: var(--fbz-color-text-muted);
  }
}

.login-form {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 8px;

  label {
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
    color: var(--fbz-color-text-soft);
  }
}

.control-wrap {
  position: relative;
  display: flex;
  align-items: center;
}

.lead-icon {
  position: absolute;
  left: 11px;
  width: 17px;
  height: 17px;
  color: var(--fbz-color-text-muted);
  pointer-events: none;
  transition: color var(--fbz-motion-fast);
}

.control-input {
  width: 100%;
  height: 42px;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
  border-radius: var(--fbz-radius-control);
  padding: 0 var(--fbz-space-3);
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-md);
  transition:
    border-color var(--fbz-motion-fast),
    box-shadow var(--fbz-motion-fast),
    background var(--fbz-motion-fast);

  &.has-icon {
    padding-left: 36px;
  }
  &.has-trail {
    padding-right: 40px;
  }

  &::placeholder {
    color: var(--fbz-color-text-disabled);
  }

  &:hover {
    border-color: var(--fbz-color-line-bright);
  }

  &:focus {
    outline: none;
    border-color: var(--fbz-color-brand-500);
    box-shadow: var(--fbz-shadow-focus);
    background: var(--fbz-color-panel-strong);
  }

  &:focus + .lead-icon,
  &:focus ~ .lead-icon {
    color: var(--fbz-color-brand-500);
  }
}

.control-wrap:focus-within .lead-icon {
  color: var(--fbz-color-brand-500);
}

.trail-btn {
  position: absolute;
  right: 6px;
  display: grid;
  place-content: center;
  width: 30px;
  height: 30px;
  border: 0;
  border-radius: var(--fbz-radius-control);
  background: transparent;
  color: var(--fbz-color-text-muted);
  cursor: pointer;
  transition:
    color var(--fbz-motion-fast),
    background var(--fbz-motion-fast);

  svg {
    width: 18px;
    height: 18px;
  }

  &:hover {
    color: var(--fbz-color-text);
    background: var(--fbz-color-panel-elevated);
  }

  &:focus-visible {
    outline: none;
    box-shadow: var(--fbz-shadow-focus);
  }
}

.form-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: -2px;
}

.remember {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  cursor: pointer;
  user-select: none;

  .remember-text {
    font-size: var(--fbz-font-size-sm);
    color: var(--fbz-color-text-soft);
  }
}

.glow-switch {
  position: relative;
  display: inline-block;
  width: 38px;
  height: 20px;
  flex-shrink: 0;

  input {
    opacity: 0;
    width: 0;
    height: 0;
  }

  .switch-slide-thumb {
    position: absolute;
    inset: 0;
    background-color: var(--fbz-color-line-bright);
    border-radius: 20px;
    transition: background-color var(--fbz-motion-fast);

    &::before {
      position: absolute;
      content: "";
      height: 14px;
      width: 14px;
      left: 3px;
      bottom: 3px;
      background-color: #fff;
      border-radius: 50%;
      transition: transform var(--fbz-motion-fast);
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
    }
  }

  input:checked + .switch-slide-thumb {
    background-color: var(--fbz-color-brand-500);

    &::before {
      transform: translateX(18px);
    }
  }

  input:focus-visible + .switch-slide-thumb {
    box-shadow: var(--fbz-shadow-focus);
  }
}

.link-btn {
  border: 0;
  background: transparent;
  padding: 0;
  font-size: var(--fbz-font-size-sm);
  color: var(--fbz-color-text-muted);
  cursor: pointer;
  transition: color var(--fbz-motion-fast);

  &:hover {
    color: var(--fbz-color-brand-500);
  }
}

.submit-btn {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  height: 44px;
  margin-top: var(--fbz-space-2);
  border: 0;
  border-radius: var(--fbz-radius-control);
  background: var(--fbz-color-brand-500);
  color: #07120a;
  font-size: var(--fbz-font-size-md);
  font-weight: 800;
  letter-spacing: 2px;
  cursor: pointer;
  transition:
    background var(--fbz-motion-fast),
    box-shadow var(--fbz-motion-base),
    transform var(--fbz-motion-fast);

  &:hover:not(:disabled) {
    background: var(--fbz-color-brand-600);
    box-shadow: 0 0 18px color-mix(in srgb, var(--fbz-color-brand-500) 38%, transparent);
  }

  &:active:not(:disabled) {
    transform: translateY(1px);
  }

  &:focus-visible {
    outline: none;
    box-shadow: var(--fbz-shadow-focus);
  }

  &:disabled {
    cursor: progress;
    opacity: 0.85;
  }
}

.spinner {
  width: 16px;
  height: 16px;
  border-radius: 50%;
  border: 2px solid color-mix(in srgb, #07120a 35%, transparent);
  border-top-color: #07120a;
  animation: spin 0.7s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.panel-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--fbz-space-3);
  margin-top: var(--fbz-space-6);
  padding-top: var(--fbz-space-4);
  border-top: 1px solid var(--fbz-color-line-soft);

  .lang-group {
    display: inline-flex;
    align-items: center;
    gap: var(--fbz-space-2);
  }

  .foot-label {
    font-size: var(--fbz-font-size-xs);
    color: var(--fbz-color-text-muted);
  }

  .lang-select {
    width: 116px;
  }

  .version {
    font-family: var(--fbz-font-display);
    font-size: var(--fbz-font-size-xs);
    letter-spacing: 0.5px;
    color: var(--fbz-color-text-disabled);
  }
}

/* 尊重减少动态偏好 */
@media (prefers-reduced-motion: reduce) {
  .poster-col.drift-up,
  .poster-col.drift-down {
    animation: none;
  }
  .spinner {
    animation-duration: 1.4s;
  }
}
</style>
