<script setup lang="ts">
import { useAuthStore } from "@/stores/auth.ts";
import { useLibraryStore } from "@/stores/library.ts";
import { useUiStore } from "@/stores/ui.ts";

const route = useRoute();
const router = useRouter();
const authStore = useAuthStore();
const libraryStore = useLibraryStore();
const uiStore = useUiStore();

const isCreate = computed(() => route.name === "admin-users-create");
const userId = computed(() => route.params.id as string);

const username = ref("");
const password = ref("");
const role = ref<"admin" | "user" | "guest">("user");
const active = ref(true);
const selectedLibraries = ref<string[]>([]);

const roleOptions = [
  { label: "超级管理员 (最高权限)", value: "admin" },
  { label: "标准点播用户 (仅点播播放)", value: "user" },
  { label: "访客账号 (只读预览，禁止播放)", value: "guest" },
];

onMounted(() => {
  if (isCreate.value) {
    username.value = "";
    password.value = "";
    role.value = "user";
    active.value = true;
    selectedLibraries.value = libraryStore.libraries.map((l) => l.id);
  } else {
    const user = authStore.users.find((u) => u.id === userId.value);
    if (user) {
      username.value = user.username;
      role.value = user.role;
      active.value = user.active;
      selectedLibraries.value = [...user.libraries];
    } else {
      uiStore.showToast("找不到该用户！", "error");
      router.push("/admin/users");
    }
  }
});

function handleSave() {
  if (!username.value.trim()) {
    uiStore.showToast("请输入用户名！", "warning");
    return;
  }

  if (isCreate.value) {
    if (!password.value) {
      uiStore.showToast("请输入登录密码！", "warning");
      return;
    }
    authStore.addUser({
      username: username.value.trim(),
      role: role.value,
      active: active.value,
      libraries: selectedLibraries.value,
    });
  } else {
    authStore.updateUser(userId.value, {
      username: username.value.trim(),
      role: role.value,
      active: active.value,
      libraries: selectedLibraries.value,
    });
    if (password.value) {
      uiStore.showToast(`用户【${username.value}】的密码已重置。`, "success");
    }
  }

  router.push("/admin/users");
}

function handleCancel() {
  router.push("/admin/users");
}
</script>

<template>
  <div class="admin-user-edit-view">
    <div class="style-settings-stack">
      <section class="settings-card" :aria-labelledby="isCreate ? 'title-create' : 'title-edit'">
        <div class="card-header">
          <span class="indicator" />
          <h3 :id="isCreate ? 'title-create' : 'title-edit'">
            {{ isCreate ? "创建系统用户" : "用户详情与权限编辑" }}
          </h3>
        </div>

        <div class="card-body">
          <p class="settings-hint">
            {{
              isCreate
                ? "创建自托管系统的新访问账号及相应的媒体库授权。"
                : "修改系统用户基本信息、系统控制权限组和授权媒体库。"
            }}
          </p>

          <div class="form-grid">
            <div class="form-group">
              <label for="edit-username">登录用户名</label>
              <input
                id="edit-username"
                v-model="username"
                type="text"
                placeholder="输入用户名"
                class="control-input"
                :disabled="!isCreate && username === 'admin'"
              />
            </div>

            <div class="form-group">
              <label for="edit-password">
                {{ isCreate ? "登录密码" : "重置密码 (留空则不修改)" }}
              </label>
              <input
                id="edit-password"
                v-model="password"
                type="password"
                :placeholder="isCreate ? '输入登录密码' : '重置新密码'"
                class="control-input"
              />
            </div>

            <div class="form-group">
              <label id="edit-role-label">系统权限角色组</label>
              <BaseSelect
                v-model="role"
                :options="roleOptions"
                aria-labelledby="edit-role-label"
                class="w-full-select"
                :disabled="!isCreate && username === 'admin'"
              />
            </div>

            <div class="toggle-row-item">
              <div class="toggle-text">
                <span class="title">启用此用户账户</span>
                <span class="desc">若禁用，该用户将无法点播流媒体或访问控制台。</span>
              </div>
              <label class="glow-switch">
                <input
                  type="checkbox"
                  v-model="active"
                  :disabled="!isCreate && username === 'admin'"
                />
                <span class="switch-slide-thumb" />
              </label>
            </div>

            <div class="form-group full-width">
              <label>授权可访问的媒体库</label>
              <div class="libraries-checklist">
                <label
                  v-for="lib in libraryStore.libraries"
                  :key="lib.id"
                  class="lib-checkbox-label"
                >
                  <input type="checkbox" :value="lib.id" v-model="selectedLibraries" />
                  <span class="custom-chk" />
                  <span>{{ lib.name }}</span>
                </label>
              </div>
            </div>
          </div>
        </div>

        <div class="card-actions-row">
          <button type="button" class="footer-btn secondary" @click="handleCancel">取消</button>
          <button type="button" class="footer-btn primary" @click="handleSave">保存配置</button>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped lang="scss">
.admin-user-edit-view {
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

.form-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--fbz-space-4);

  @media (max-width: 768px) {
    grid-template-columns: 1fr;
  }

  .full-width {
    grid-column: span 2;

    @media (max-width: 768px) {
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

    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }

  .w-full-select {
    width: 100%;
    :deep(.trigger) {
      background: var(--fbz-color-panel);
    }
  }
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

.libraries-checklist {
  display: flex;
  flex-direction: column;
  gap: 10px;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-control);
  padding: 12px 14px;
  max-height: 180px;
  overflow-y: auto;
}

.lib-checkbox-label {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: var(--fbz-font-size-sm);
  cursor: pointer;
  user-select: none;

  input {
    opacity: 0;
    position: absolute;
    width: 0;
    height: 0;
  }

  .custom-chk {
    width: 16px;
    height: 16px;
    border: 1px solid var(--fbz-color-line);
    border-radius: 4px;
    background: var(--fbz-color-panel-strong);
    display: grid;
    place-content: center;
    transition: all var(--fbz-motion-fast);

    &::after {
      content: "✓";
      color: #07120a;
      font-size: 10px;
      font-weight: 900;
      opacity: 0;
      transition: opacity var(--fbz-motion-fast);
    }
  }

  input:checked + .custom-chk {
    border-color: var(--fbz-color-brand-500);
    background-color: var(--fbz-color-brand-500);

    &::after {
      opacity: 1;
    }
  }

  input:focus-visible + .custom-chk {
    outline: none;
    box-shadow: var(--fbz-shadow-focus);
  }
}

.card-actions-row {
  padding: var(--fbz-space-4) var(--fbz-space-5);
  border-top: 1px solid var(--fbz-color-line-soft);
  display: flex;
  justify-content: flex-end;
  gap: var(--fbz-space-3);
  background: var(--fbz-color-bg-strong);
}

.footer-btn {
  height: 36px;
  padding: 0 var(--fbz-space-5);
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  border-radius: var(--fbz-radius-control);
  cursor: pointer;
  transition: all var(--fbz-motion-fast);

  &.primary {
    background: var(--fbz-color-brand-500);
    border: 0;
    color: #07120a;

    &:hover {
      background: var(--fbz-color-brand-600);
      box-shadow: 0 0 12px color-mix(in srgb, var(--fbz-color-brand-500) 30%, transparent);
    }
  }

  &.secondary {
    background: var(--fbz-color-panel-strong);
    border: 1px solid var(--fbz-color-line);
    color: var(--fbz-color-text-soft);

    &:hover {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
      border-color: var(--fbz-color-line-bright);
    }
  }
}
</style>
