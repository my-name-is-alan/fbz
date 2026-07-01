<script setup lang="ts">
import { useAuthStore } from "@/stores/auth.ts";

const router = useRouter();
const authStore = useAuthStore();

// 列表内头像的缓存键：进页面时固定一次即可（他人头像不在本会话内频繁变动）。
const avatarCacheKey = Date.now();

// 进入用户管理页时从后端拉取真实用户列表。
onMounted(() => {
  authStore.loadUsers();
});

function openCreateUser() {
  router.push("/admin/users/create");
}

function openEditUser(userId: string) {
  router.push(`/admin/users/${userId}`);
}

function formatLastLogin(value: string | null): string {
  if (!value) return "从未登录";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}
</script>

<template>
  <div class="admin-users-view">
    <div class="users-header">
      <p class="settings-hint">管理自托管系统的访问账号及媒体库浏览权限。</p>
      <button class="add-user-btn" type="button" @click="openCreateUser">➕ 添加系统用户</button>
    </div>

    <div class="users-list-grid">
      <div
        v-for="user in authStore.users"
        :key="user.id"
        class="user-card"
        :class="{ disabled: !user.active }"
      >
        <div class="user-card-top">
          <BaseAvatar
            :user-id="user.id"
            :name="user.displayName || user.username"
            :version="avatarCacheKey"
            :size="44"
            :class="{ 'is-disabled': !user.active }"
          />
          <div class="user-meta-info">
            <div class="username-row">
              <span class="username">{{ user.username }}</span>
              <span class="role-badge" :class="user.role">{{ user.roleLabel }}</span>
            </div>
            <p class="desc-text">
              {{ user.displayName || user.desc }}
            </p>
          </div>
        </div>
        <div class="user-facts">
          <span>设备 {{ user.deviceCount }}</span>
          <span>会话 {{ user.activeSessionCount }}</span>
          <span>{{ user.allowDownload ? "允许下载" : "禁止下载" }}</span>
          <span>{{ user.allowTranscode ? "允许转码" : "禁止转码" }}</span>
          <span>{{ user.allowNewDeviceLogin ? "允许新设备" : "锁定新设备" }}</span>
          <span>最近 {{ formatLastLogin(user.lastLoginAt) }}</span>
        </div>
        <div class="user-card-footer">
          <button
            class="action-btn text-btn"
            type="button"
            @click="authStore.toggleUserStatus(user.id)"
          >
            {{ user.active ? "禁用账号" : "启用账号" }}
          </button>
          <button class="action-btn text-btn" type="button" @click="openEditUser(user.id)">
            编辑权限
          </button>
          <div class="spacer" />
          <button
            class="action-btn danger-btn"
            type="button"
            @click="authStore.deleteUser(user.id)"
          >
            删除
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.admin-users-view {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.users-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--fbz-space-4);
  margin-bottom: var(--fbz-space-2);

  @media (max-width: 600px) {
    flex-direction: column;
    align-items: flex-start;
  }

  .settings-hint {
    margin: 0;
    font-size: var(--fbz-font-size-sm);
    color: var(--fbz-color-text-muted);
  }

  .add-user-btn {
    height: 34px;
    padding: 0 14px;
    background: var(--fbz-color-brand-500);
    border: 0;
    color: #07120a;
    font-weight: 700;
    font-size: var(--fbz-font-size-sm);
    border-radius: var(--fbz-radius-control);
    cursor: pointer;
    transition: all var(--fbz-motion-fast);
    flex-shrink: 0;

    &:hover {
      background: var(--fbz-color-brand-600);
    }
  }
}

.users-list-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: var(--fbz-space-3);
}

.user-card {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  padding: var(--fbz-space-4);
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  gap: 16px;
  transition: all var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-brand-500);
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  }

  &.disabled {
    opacity: 0.6;
  }

  .user-card-top {
    display: flex;
    gap: 14px;

    .base-avatar.is-disabled {
      filter: grayscale(1);
      opacity: 0.6;
    }

    .user-meta-info {
      display: flex;
      flex-direction: column;
      gap: 6px;
      min-width: 0;

      .username-row {
        display: flex;
        align-items: center;
        gap: 8px;
        flex-wrap: wrap;

        .username {
          font-size: var(--fbz-font-size-md);
          font-weight: 700;
          color: var(--fbz-color-text);
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
        }

        .role-badge {
          font-size: 9px;
          font-weight: 800;
          padding: 1px 6px;
          border-radius: 3px;
          text-transform: uppercase;

          &.admin {
            background: color-mix(in srgb, var(--fbz-color-brand-500) 12%, transparent);
            color: var(--fbz-color-brand-500);
            border: 1px solid color-mix(in srgb, var(--fbz-color-brand-500) 25%, transparent);
          }

          &.user {
            background: color-mix(in srgb, var(--fbz-color-cyan-500) 12%, transparent);
            color: var(--fbz-color-cyan-500);
            border: 1px solid color-mix(in srgb, var(--fbz-color-cyan-500) 25%, transparent);
          }

          &.guest {
            background: var(--fbz-color-line);
            color: var(--fbz-color-text-muted);
            border: 1px solid var(--fbz-color-line-bright);
          }
        }
      }

      .desc-text {
        margin: 0;
        font-size: var(--fbz-font-size-xs);
        color: var(--fbz-color-text-muted);
        line-height: 1.4;
      }
    }
  }

  .user-card-footer {
    display: flex;
    align-items: center;
    border-top: 1px solid var(--fbz-color-line-soft);
    padding-top: 10px;
    gap: 8px;

    .spacer {
      flex: 1;
    }

    .action-btn {
      height: 26px;
      padding: 0 8px;
      font-size: 11px;
      font-weight: 700;
      border-radius: 4px;
      cursor: pointer;
      transition: all var(--fbz-motion-fast);

      &.text-btn {
        background: var(--fbz-color-panel);
        border: 1px solid var(--fbz-color-line);
        color: var(--fbz-color-text-soft);

        &:hover {
          background: var(--fbz-color-panel-elevated);
          color: var(--fbz-color-text);
        }
      }

      &.danger-btn {
        background: transparent;
        border: 1px solid var(--fbz-color-danger-500);
        color: var(--fbz-color-danger-500);

        &:hover:not(:disabled) {
          background: color-mix(in srgb, var(--fbz-color-danger-500) 8%, transparent);
        }

        &:disabled {
          opacity: 0.3;
          cursor: not-allowed;
        }
      }
    }
  }
}

.user-facts {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;

  span {
    border: 1px solid var(--fbz-color-line);
    border-radius: 4px;
    padding: 2px 6px;
    background: var(--fbz-color-panel);
    color: var(--fbz-color-text-muted);
    font-size: 10px;
    font-weight: 700;
  }
}
</style>
