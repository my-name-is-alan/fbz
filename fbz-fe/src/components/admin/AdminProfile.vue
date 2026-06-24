<script setup lang="ts">
import { useAuthStore } from "@/stores/auth.ts";
import { useUiStore } from "@/stores/ui.ts";

const authStore = useAuthStore();
const uiStore = useUiStore();

const formNickname = ref(authStore.nickname);
const formUsername = ref(authStore.username);
const formEmail = ref(authStore.email);

const currentPassword = ref("");
const newPassword = ref("");
const confirmPassword = ref("");

const formLanguage = ref(authStore.language);
const formAutoSub = ref(authStore.autoSubtitles);
const formAudioPref = ref(authStore.audioPreference);

const languageOptions = [
  { label: "简体中文 (Chinese Simplified)", value: "zh-CN" },
  { label: "繁體中文 (Chinese Traditional)", value: "zh-TW" },
  { label: "English (United States)", value: "en-US" },
  { label: "日本語 (Japanese)", value: "ja-JP" },
];

const audioOptions = [
  { label: "中文 (Chinese)", value: "zh" },
  { label: "英语 (English)", value: "en" },
  { label: "日语 (Japanese)", value: "ja" },
  { label: "原声优先 (Original Soundtrack)", value: "original" },
];

function handleSaveProfile() {
  const success = authStore.updateProfile({
    nickname: formNickname.value,
    username: formUsername.value,
    email: formEmail.value,
    language: formLanguage.value,
    autoSubtitles: formAutoSub.value,
    audioPreference: formAudioPref.value,
  });

  if (success) {
    currentPassword.value = "";
    newPassword.value = "";
    confirmPassword.value = "";
  }
}

function handlePasswordChange() {
  if (newPassword.value !== confirmPassword.value) {
    uiStore.showToast("新密码与确认密码不一致！", "warning");
    return;
  }
  const success = authStore.changePassword(currentPassword.value, newPassword.value);
  if (success) {
    currentPassword.value = "";
    newPassword.value = "";
    confirmPassword.value = "";
  }
}
</script>

<template>
  <div class="admin-profile-view">
    <div class="style-settings-stack">
      <section class="settings-card" aria-labelledby="section-profile-title">
        <div class="card-header">
          <span class="indicator" />
          <h3 id="section-profile-title">基本信息</h3>
        </div>
        <div class="card-body">
          <p class="settings-hint">更新您的头像昵称、系统用户名和用于找回密码的电子邮箱。</p>
          <div class="profile-avatar-row">
            <div class="profile-avatar-large">
              {{ formNickname.charAt(0).toUpperCase() }}
            </div>
            <div class="avatar-meta">
              <span class="avatar-title">系统头像</span>
              <span class="avatar-desc">根据昵称的首字符自动生成</span>
            </div>
          </div>

          <div class="profile-form-grid">
            <div class="form-group">
              <label for="profile-nickname">显示昵称</label>
              <input
                id="profile-nickname"
                v-model="formNickname"
                type="text"
                placeholder="请输入您的昵称"
                class="control-input"
              />
            </div>

            <div class="form-group">
              <label for="profile-username">账户登录名</label>
              <input
                id="profile-username"
                v-model="formUsername"
                type="text"
                placeholder="请输入登录用户名"
                class="control-input"
              />
            </div>

            <div class="form-group full-width">
              <label for="profile-email">电子邮箱地址</label>
              <input
                id="profile-email"
                v-model="formEmail"
                type="email"
                placeholder="请输入电子邮箱"
                class="control-input"
              />
            </div>
          </div>
        </div>
      </section>

      <section class="settings-card" aria-labelledby="section-pref-title">
        <div class="card-header">
          <span class="indicator" />
          <h3 id="section-pref-title">偏好设置</h3>
        </div>
        <div class="card-body">
          <p class="settings-hint">选择您在点播界面时偏好的显示语言、默认音频和字幕轨道策略。</p>

          <div class="preferences-grid">
            <div class="form-group">
              <label id="pref-lang-lbl">界面语言偏好</label>
              <BaseSelect
                v-model="formLanguage"
                :options="languageOptions"
                aria-labelledby="pref-lang-lbl"
                class="w-full-select"
              />
            </div>

            <div class="form-group">
              <label id="pref-audio-lbl">默认音轨语言</label>
              <BaseSelect
                v-model="formAudioPref"
                :options="audioOptions"
                aria-labelledby="pref-audio-lbl"
                class="w-full-select"
              />
            </div>

            <div class="toggle-row-item">
              <div class="toggle-text">
                <span class="title">默认自动加载字幕</span>
                <span class="desc">播放视频时，若有匹配您语言的字幕，则默认自动启用。</span>
              </div>
              <label class="glow-switch">
                <input type="checkbox" v-model="formAutoSub" />
                <span class="switch-slide-thumb" />
              </label>
            </div>
          </div>
        </div>
      </section>

      <div class="actions-footer">
        <button type="button" class="save-profile-btn" @click="handleSaveProfile">
          保存所有修改
        </button>
      </div>

      <section class="settings-card" aria-labelledby="section-password-title">
        <div class="card-header">
          <span class="indicator dev-indicator" />
          <h3 id="section-password-title">安全与密码</h3>
        </div>
        <div class="card-body">
          <p class="settings-hint">为保障您的媒体库安全，建议定期更新登录凭证密码。</p>

          <div class="profile-form-grid">
            <div class="form-group">
              <label for="pass-current">当前登录密码</label>
              <input
                id="pass-current"
                v-model="currentPassword"
                type="password"
                placeholder="请输入当前正在使用的密码"
                class="control-input"
              />
            </div>

            <div class="form-group">
              <label for="pass-new">新登录密码</label>
              <input
                id="pass-new"
                v-model="newPassword"
                type="password"
                placeholder="新密码最少 6 位"
                class="control-input"
              />
            </div>

            <div class="form-group">
              <label for="pass-confirm">确认新密码</label>
              <input
                id="pass-confirm"
                v-model="confirmPassword"
                type="password"
                placeholder="请再次输入新密码"
                class="control-input"
              />
            </div>
          </div>
        </div>
        <div class="card-actions-row">
          <button type="button" class="change-password-btn" @click="handlePasswordChange">
            更改登录密码
          </button>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped lang="scss">
.admin-profile-view {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.style-settings-stack {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.settings-card {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
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
  margin: 0 0 var(--fbz-space-2);
  font-size: var(--fbz-font-size-sm);
  color: var(--fbz-color-text-muted);
}

.profile-avatar-row {
  display: flex;
  align-items: center;
  gap: 16px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--fbz-color-line-soft);

  .profile-avatar-large {
    width: 60px;
    height: 60px;
    border-radius: 50%;
    background: var(--fbz-color-brand-500);
    color: #07120a;
    font-weight: 800;
    font-size: 24px;
    display: grid;
    place-content: center;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  }

  .avatar-meta {
    display: flex;
    flex-direction: column;
    gap: 4px;

    .avatar-title {
      font-size: var(--fbz-font-size-sm);
      font-weight: 700;
      color: var(--fbz-color-text);
    }

    .avatar-desc {
      font-size: var(--fbz-font-size-xs);
      color: var(--fbz-color-text-muted);
    }
  }
}

.profile-form-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--fbz-space-4);

  @media (max-width: 600px) {
    grid-template-columns: 1fr;
  }

  .full-width {
    grid-column: span 2;

    @media (max-width: 600px) {
      grid-column: span 1;
    }
  }
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

  .control-input {
    height: 38px;
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    padding: 0 var(--fbz-space-3);
    color: var(--fbz-color-text);
    font-size: var(--fbz-font-size-sm);
    transition: all var(--fbz-motion-fast);

    &:focus {
      outline: none;
      border-color: var(--fbz-color-brand-500);
      box-shadow: var(--fbz-shadow-focus);
    }
  }

  .w-full-select {
    width: 100%;
    :deep(.trigger) {
      background: var(--fbz-color-panel);
    }
  }
}

.preferences-grid {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.toggle-row-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--fbz-space-6);
  padding: 12px 14px;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-control);

  .toggle-text {
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
  cursor: pointer;

  input {
    opacity: 0;
    width: 0;
    height: 0;
  }

  .switch-slide-thumb {
    position: absolute;
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
    outline: none;
    box-shadow: var(--fbz-shadow-focus);
  }
}

.actions-footer {
  display: flex;
  justify-content: flex-end;
  padding: var(--fbz-space-2) 0;

  .save-profile-btn {
    height: 38px;
    padding: 0 var(--fbz-space-6);
    background: var(--fbz-color-brand-500);
    border: 0;
    color: #07120a;
    font-weight: 700;
    font-size: var(--fbz-font-size-sm);
    border-radius: var(--fbz-radius-control);
    cursor: pointer;
    transition: all var(--fbz-motion-fast);

    &:hover {
      background: var(--fbz-color-brand-600);
      box-shadow: 0 0 12px color-mix(in srgb, var(--fbz-color-brand-500) 30%, transparent);
    }
  }
}

.card-actions-row {
  padding: var(--fbz-space-3) var(--fbz-space-5);
  border-top: 1px solid var(--fbz-color-line-soft);
  display: flex;
  justify-content: flex-end;
  background: var(--fbz-color-bg-strong);

  .change-password-btn {
    height: 36px;
    padding: 0 var(--fbz-space-5);
    background: var(--fbz-color-panel-strong);
    border: 1px solid var(--fbz-color-line);
    color: var(--fbz-color-text-soft);
    font-weight: 700;
    font-size: var(--fbz-font-size-sm);
    border-radius: var(--fbz-radius-control);
    cursor: pointer;
    transition: all var(--fbz-motion-fast);

    &:hover {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
      border-color: var(--fbz-color-line-bright);
    }
  }
}
</style>
