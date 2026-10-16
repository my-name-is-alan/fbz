<script setup lang="ts">
import { useAuthStore } from "@/stores/auth.ts";
import { useUiStore } from "@/stores/ui.ts";
import { useCSurface } from "@/composables/useCSurface.ts";

useCSurface();

const router = useRouter();
const route = useRoute();
const authStore = useAuthStore();
const uiStore = useUiStore();

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

const features = ["Emby 客户端兼容", "多库 · 多版本", "转码与直放", "多用户权限"];

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
    <section class="login-art" aria-hidden="true">
      <div class="art-grid">
        <span v-for="n in 24" :key="n" class="tile" :class="`t${n % 5}`" />
      </div>
      <div class="art-veil" />
      <div class="art-brand">
        <div class="brand-mark">
          <span class="brand-ic"><BaseIcon name="clapperboard" :size="22" /></span>
          <span class="brand-word">FBZ</span>
        </div>
        <h1 class="art-title">自托管媒体中心</h1>
        <p class="art-sub">
          扫描、刮削、转码与串流，统一管理你的本地与云端影视、音乐与图书媒体库。
        </p>
        <ul class="art-features">
          <li v-for="f in features" :key="f"><BaseIcon name="check" :size="14" />{{ f }}</li>
        </ul>
      </div>
    </section>

    <section class="login-panel">
      <div class="panel-inner">
        <div class="mobile-brand">
          <span class="brand-ic"><BaseIcon name="clapperboard" :size="18" /></span>
          <span class="brand-word">FBZ</span>
        </div>

        <header class="panel-head">
          <h2>登录到 FBZ</h2>
          <p>输入账户凭据以进入媒体工作台。</p>
        </header>

        <form class="login-form" @submit.prevent="handleLogin">
          <label class="field" for="login-username">
            <span class="field-label">用户名</span>
            <span class="control">
              <BaseIcon name="user" :size="16" class="lead" />
              <input
                id="login-username"
                v-model="username"
                type="text"
                placeholder="请输入登录用户名"
                autocomplete="username"
              />
            </span>
          </label>

          <label class="field" for="login-password">
            <span class="field-label">登录密码</span>
            <span class="control">
              <BaseIcon name="lock" :size="16" class="lead" />
              <input
                id="login-password"
                v-model="password"
                :type="showPassword ? 'text' : 'password'"
                placeholder="请输入密码"
                autocomplete="current-password"
              />
              <button
                type="button"
                class="trail"
                :aria-label="showPassword ? '隐藏密码' : '显示密码'"
                @click="showPassword = !showPassword"
              >
                <BaseIcon :name="showPassword ? 'eye-off' : 'eye'" :size="16" />
              </button>
            </span>
          </label>

          <div class="form-row">
            <label class="remember">
              <input
                v-model="remember"
                class="du-toggle du-toggle-sm du-toggle-primary"
                type="checkbox"
              />
              <span>记住此设备</span>
            </label>
            <button type="button" class="link-btn" @click="handleForgot">忘记密码？</button>
          </div>

          <button type="submit" class="du-btn du-btn-primary submit-btn" :disabled="loading">
            <BaseIcon
              :name="loading ? 'loading' : 'key'"
              :size="16"
              :class="{ 'c-spin': loading }"
            />
            {{ loading ? "正在登录…" : "登录" }}
          </button>
        </form>

        <footer class="panel-foot">
          <label class="lang-group">
            <BaseIcon name="languages" :size="15" />
            <BaseSelect
              v-model="language"
              :options="languageOptions"
              size="sm"
              aria-label="界面语言"
            />
          </label>
          <span class="version">{{ appVersion }}</span>
        </footer>
      </div>
    </section>
  </div>
</template>

<style scoped lang="scss">
.login-view {
  min-height: 100vh;
  display: grid;
  grid-template-columns: minmax(0, 1.2fr) minmax(380px, 0.8fr);
  background: var(--ui-background);
}

/* ---------- 左侧品牌区 ---------- */
.login-art {
  position: relative;
  overflow: hidden;
  border-right: 1px solid var(--ui-border);
}

.art-grid {
  position: absolute;
  inset: -40px;
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: 14px;
  transform: rotate(-8deg) scale(1.15);
  opacity: 0.55;
}

.tile {
  aspect-ratio: 2 / 3;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-surface-raised);

  &.t1 {
    background: linear-gradient(
      160deg,
      rgb(var(--ui-accent-channel) / 18%),
      var(--ui-surface-raised)
    );
  }

  &.t2 {
    background: var(--ui-surface);
  }

  &.t3 {
    background: linear-gradient(200deg, var(--ui-surface-hover), var(--ui-surface));
  }
}

.art-veil {
  position: absolute;
  inset: 0;
  background:
    radial-gradient(circle at 20% 80%, rgb(var(--ui-accent-channel) / 14%), transparent 50%),
    linear-gradient(
      90deg,
      rgb(var(--ui-scrim) / 92%) 0%,
      rgb(var(--ui-scrim) / 70%) 60%,
      rgb(var(--ui-scrim) / 88%) 100%
    );
}

.art-brand {
  position: absolute;
  left: 56px;
  right: 56px;
  bottom: 64px;
  max-width: 520px;
}

.brand-mark,
.mobile-brand {
  display: inline-flex;
  align-items: center;
  gap: 10px;
}

.brand-ic {
  width: 40px;
  height: 40px;
  display: grid;
  place-items: center;
  border-radius: var(--ui-radius-md);
  background: var(--ui-accent);
  color: var(--ui-accent-contrast);
  box-shadow: var(--ui-shadow-accent);
}

.brand-word {
  color: var(--ui-text);
  font-size: 24px;
  font-weight: 700;
  letter-spacing: -0.02em;
}

.art-title {
  margin: 24px 0 8px;
  color: var(--ui-text);
  font-size: 34px;
  font-weight: 700;
  letter-spacing: -0.02em;
}

.art-sub {
  margin: 0;
  color: var(--ui-text-muted);
  font-size: 14px;
  line-height: 1.7;
}

.art-features {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin: 20px 0 0;
  padding: 0;
  list-style: none;

  li {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    border: 1px solid var(--ui-border-strong);
    border-radius: var(--ui-radius-sm);
    background: rgb(var(--ui-scrim) / 60%);
    color: var(--ui-text);
    font-size: 12.5px;

    :deep(.fbz-icon) {
      color: var(--ui-accent);
    }
  }
}

/* ---------- 右侧表单 ---------- */
.login-panel {
  display: grid;
  place-items: center;
  padding: 40px 24px;
  background: var(--ui-surface);
}

.panel-inner {
  width: 100%;
  max-width: 380px;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.mobile-brand {
  display: none;

  .brand-ic {
    width: 34px;
    height: 34px;
  }

  .brand-word {
    font-size: 20px;
  }
}

.panel-head {
  h2 {
    margin: 0 0 6px;
    color: var(--ui-text);
    font-size: 22px;
    font-weight: 600;
    letter-spacing: -0.02em;
  }

  p {
    margin: 0;
    color: var(--ui-text-muted);
    font-size: 13px;
  }
}

.login-form {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field-label {
  color: var(--ui-text-muted);
  font-size: 12px;
  font-weight: 600;
}

.control {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 44px;
  padding: 0 12px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-input);
  transition:
    border-color var(--ui-duration-fast),
    box-shadow var(--ui-duration-fast);

  &:focus-within {
    border-color: rgb(var(--ui-accent-channel) / 60%);
    box-shadow: 0 0 0 3px rgb(var(--ui-accent-channel) / 12%);
  }

  .lead {
    color: var(--ui-text-subtle);
  }

  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--ui-text);
    font-size: 14px;

    &::placeholder {
      color: var(--ui-text-subtle);
    }
  }
}

.trail {
  width: 30px;
  height: 30px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  color: var(--ui-text-subtle);
  cursor: pointer;

  &:hover {
    color: var(--ui-text);
  }
}

.form-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.remember {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  color: var(--ui-text-muted);
  font-size: 13px;
  cursor: pointer;
}

.link-btn {
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--ui-accent);
  font-size: 13px;
  cursor: pointer;

  &:hover {
    color: var(--ui-accent-hover);
  }
}

.submit-btn {
  height: 44px;
  min-height: 44px;
  border-radius: var(--ui-radius-md);
  font-size: 14px;
}

.panel-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding-top: 16px;
  border-top: 1px solid var(--ui-border);
}

.lang-group {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--ui-text-subtle);

  :deep(.base-select) {
    min-width: 128px;
  }
}

.version {
  color: var(--ui-text-subtle);
  font-family: var(--ui-font-mono);
  font-size: 11px;
}

@media (max-width: 900px) {
  .login-view {
    grid-template-columns: 1fr;
  }

  .login-art {
    display: none;
  }

  .mobile-brand {
    display: inline-flex;
  }
}
</style>
