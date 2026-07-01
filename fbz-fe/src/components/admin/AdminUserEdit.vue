<script setup lang="ts">
import { useAuthStore } from "@/stores/auth.ts";
import { useLibraryStore } from "@/stores/library.ts";
import { useUiStore } from "@/stores/ui.ts";
import {
  listUserLibraryPermissions,
  updateUserLibraryPermission,
} from "@/service/modules/admin.ts";
import type { UserLibraryPermission } from "@/types/admin.ts";

const route = useRoute();
const router = useRouter();
const authStore = useAuthStore();
const libraryStore = useLibraryStore();
const uiStore = useUiStore();

const isCreate = computed(() => route.name === "admin-users-create");
const userId = computed(() => route.params.id as string);

/** 后端策略：管理员/用户密码至少 6 位。 */
const PASSWORD_MIN_LEN = 6;

const username = ref("");
const password = ref("");
const role = ref<"admin" | "user" | "guest">("user");
const active = ref(true);
const selectedLibraries = ref<string[]>([]);
const libraryPermissions = ref<UserLibraryPermission[]>([]);
const permissionsLoading = ref(false);
const saving = ref(false);

const roleOptions = [
  { label: "超级管理员 (最高权限)", value: "admin" },
  { label: "标准点播用户 (仅点播播放)", value: "user" },
  { label: "访客账号 (只读预览，禁止播放)", value: "guest" },
];

onMounted(async () => {
  if (!libraryStore.loaded) {
    await libraryStore.loadFromBackend();
  }
  if (isCreate.value) {
    username.value = "";
    password.value = "";
    role.value = "user";
    active.value = true;
    selectedLibraries.value = libraryStore.libraries.map((l) => l.id);
    return;
  }
  // 编辑态：确保用户列表已加载，再回填。
  if (authStore.users.length === 0) {
    await authStore.loadUsers();
  }
  const user = authStore.users.find((u) => u.id === userId.value);
  if (user) {
    username.value = user.username;
    role.value = user.role;
    active.value = user.active;
    selectedLibraries.value = [...user.libraries];
    await loadLibraryPermissions();
  } else {
    uiStore.showToast("找不到该用户！", "error");
    router.push("/admin/users");
  }
});

async function loadLibraryPermissions() {
  permissionsLoading.value = true;
  try {
    libraryPermissions.value = await listUserLibraryPermissions(userId.value);
  } catch {
    uiStore.showToast("加载用户媒体库权限失败。", "error");
  } finally {
    permissionsLoading.value = false;
  }
}

async function handleSave() {
  if (saving.value) return;

  if (isCreate.value) {
    if (!username.value.trim()) {
      uiStore.showToast("请输入用户名！", "warning");
      return;
    }
    if (password.value.length < PASSWORD_MIN_LEN) {
      uiStore.showToast(`登录密码至少需要 ${PASSWORD_MIN_LEN} 位！`, "warning");
      return;
    }
    saving.value = true;
    const ok = await authStore.addUser({
      username: username.value.trim(),
      password: password.value,
      role: role.value,
      active: active.value,
    });
    saving.value = false;
    if (ok) router.push("/admin/users");
    return;
  }

  // 编辑态：后端只支持改启用态（policy）。用户名/角色/密码/媒体库授权暂无运行时接口。
  saving.value = true;
  const ok = await authStore.updateUser(userId.value, { active: active.value });
  if (ok && libraryPermissions.value.length > 0) {
    try {
      await Promise.all(
        libraryPermissions.value.map((permission) =>
          updateUserLibraryPermission(userId.value, permission.libraryId, {
            canView: permission.canView,
            canDownload: permission.canDownload,
            canTranscode: permission.canTranscode,
          }),
        ),
      );
    } catch {
      saving.value = false;
      uiStore.showToast("保存媒体库授权失败。", "error");
      return;
    }
  }
  saving.value = false;
  if (ok) router.push("/admin/users");
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
                ? `创建自托管系统的新访问账号。密码至少 ${PASSWORD_MIN_LEN} 位。`
                : "编辑态支持切换账号启用状态，并按媒体库设置浏览、下载和转码权限。"
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
                :disabled="!isCreate"
              />
            </div>

            <div v-if="isCreate" class="form-group">
              <label for="edit-password">登录密码（至少 {{ PASSWORD_MIN_LEN }} 位）</label>
              <input
                id="edit-password"
                v-model="password"
                type="password"
                placeholder="输入登录密码"
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
                :disabled="!isCreate"
              />
            </div>

            <div class="toggle-row-item">
              <div class="toggle-text">
                <span class="title">启用此用户账户</span>
                <span class="desc">若禁用，该用户将无法点播流媒体或访问控制台。</span>
              </div>
              <label class="glow-switch">
                <input type="checkbox" v-model="active" />
                <span class="switch-slide-thumb" />
              </label>
            </div>

            <div v-if="isCreate" class="form-group full-width">
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

            <div v-else class="form-group full-width">
              <label>媒体库授权</label>
              <div v-if="permissionsLoading" class="permissions-empty">正在加载媒体库权限...</div>
              <div v-else-if="libraryPermissions.length === 0" class="permissions-empty">
                当前没有可授权媒体库。请先创建媒体库。
              </div>
              <div v-else class="permissions-list">
                <div
                  v-for="permission in libraryPermissions"
                  :key="permission.libraryId"
                  class="permission-row"
                >
                  <div class="permission-main">
                    <span class="permission-name">{{ permission.libraryName }}</span>
                    <span class="permission-meta">
                      {{ permission.libraryType }}
                      {{ permission.permissionConfigured ? "已配置显式权限" : "继承全局用户策略" }}
                    </span>
                  </div>
                  <label class="permission-toggle">
                    <input v-model="permission.canView" type="checkbox" />
                    <span>浏览</span>
                  </label>
                  <label class="permission-toggle">
                    <input v-model="permission.canDownload" type="checkbox" />
                    <span>下载</span>
                  </label>
                  <label class="permission-toggle">
                    <input v-model="permission.canTranscode" type="checkbox" />
                    <span>转码</span>
                  </label>
                </div>
              </div>
            </div>
          </div>
        </div>

        <div class="card-actions-row">
          <button type="button" class="footer-btn secondary" @click="handleCancel">取消</button>
          <button type="button" class="footer-btn primary" :disabled="saving" @click="handleSave">
            {{ saving ? "保存中…" : "保存配置" }}
          </button>
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

.permissions-empty {
  background: var(--fbz-color-panel);
  border: 1px dashed var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-control);
  color: var(--fbz-color-text-muted);
  padding: var(--fbz-space-4);
  font-size: var(--fbz-font-size-sm);
  text-align: center;
}

.permissions-list {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-control);
  padding: var(--fbz-space-3);
}

.permission-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) repeat(3, auto);
  align-items: center;
  gap: var(--fbz-space-3);
  padding: 10px 12px;
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-control);
  background: var(--fbz-color-panel-strong);
}

.permission-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.permission-name {
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-sm);
  font-weight: 800;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.permission-meta {
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-xs);
}

.permission-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--fbz-color-text-soft);
  font-size: var(--fbz-font-size-xs);
  font-weight: 700;
  cursor: pointer;

  input {
    accent-color: var(--fbz-color-brand-500);
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

@media (max-width: 720px) {
  .permission-row {
    grid-template-columns: 1fr;
    align-items: flex-start;
  }
}
</style>
