<script setup lang="ts">
import { useAuthStore } from "@/stores/auth.ts";

const router = useRouter();
const authStore = useAuthStore();

function openCreateUser() {
  router.push("/admin/users/create");
}

function openEditUser(userId: string) {
  router.push(`/admin/users/${userId}`);
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
          <div
            class="user-avatar-circle"
            :style="{
              background: user.active
                ? 'var(--fbz-color-brand-500)'
                : 'var(--fbz-color-text-disabled)',
            }"
          >
            {{ user.username.charAt(0).toUpperCase() }}
          </div>
          <div class="user-meta-info">
            <div class="username-row">
              <span class="username">{{ user.username }}</span>
              <span class="role-badge" :class="user.role">{{ user.roleLabel }}</span>
            </div>
            <p class="desc-text">{{ user.desc }}</p>
          </div>
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
            :disabled="user.username === 'admin'"
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

    .user-avatar-circle {
      width: 44px;
      height: 44px;
      border-radius: 50%;
      color: #07120a;
      display: grid;
      place-content: center;
      font-weight: 800;
      font-size: 18px;
      flex-shrink: 0;
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
</style>
