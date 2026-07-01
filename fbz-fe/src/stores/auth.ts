import { ref } from "vue";
import { defineStore } from "pinia";
import { isAxiosError } from "axios";
import { useUiStore } from "@/stores/ui.ts";
import { login as loginRequest, logout as logoutRequest } from "@/service/modules/auth.ts";
import {
  createSystemUser,
  deleteSystemUser,
  listSystemUsers,
  updateSystemUserPolicy,
} from "@/service/modules/admin.ts";
import { getAccessToken } from "@/service/request.ts";
import type { AdminUser } from "@/types/admin.ts";

export interface UserProfile {
  username: string;
  email: string;
  nickname: string;
  language: string;
  autoSubtitles: boolean;
  audioPreference: string;
}

export interface SystemUser {
  id: string;
  username: string;
  displayName: string | null;
  role: "admin" | "user" | "guest";
  roleLabel: string;
  active: boolean;
  desc: string;
  libraries: string[];
  allowDownload: boolean;
  allowTranscode: boolean;
  allowNewDeviceLogin: boolean;
  deviceCount: number;
  activeSessionCount: number;
  lastLoginAt: string | null;
}

export const useAuthStore = defineStore("auth", () => {
  const uiStore = useUiStore();

  const username = ref<string>(localStorage.getItem("fbz_auth_username") ?? "admin");
  const email = ref<string>(localStorage.getItem("fbz_auth_email") ?? "admin@fbz.com");
  const nickname = ref<string>(localStorage.getItem("fbz_auth_nickname") ?? "Admin");

  const language = ref<string>(localStorage.getItem("fbz_pref_language") ?? "zh-CN");
  const autoSubtitles = ref<boolean>(localStorage.getItem("fbz_pref_autosub") !== "false");
  const audioPreference = ref<string>(localStorage.getItem("fbz_pref_audiopref") ?? "zh");

  // 登录态：以本地是否持有访问令牌为准（令牌由真实登录接口签发）。
  const userId = ref<string>(localStorage.getItem("fbz_auth_user_id") ?? "");
  const isAuthenticated = ref<boolean>(getAccessToken() !== null);

  // 头像缓存版本：更换/删除头像后自增，作为 `<img src>` 的 `?v=` 击穿缓存。
  const avatarVersion = ref<number>(Number(localStorage.getItem("fbz_auth_avatar_v") ?? "0"));

  /** 头像更新后调用：自增版本并持久化，驱动所有引用处刷新。 */
  function bumpAvatarVersion(): void {
    avatarVersion.value = Date.now();
    localStorage.setItem("fbz_auth_avatar_v", String(avatarVersion.value));
  }

  // System Users List：从后端 `/api/admin/users` 拉取真实数据，不再用 localStorage mock。
  const users = ref<SystemUser[]>([]);
  const usersLoading = ref(false);
  // 保留后端原始记录，构建 policy 更新时需要完整的权限标志（避免丢失 allow* 字段）。
  const userRecords = ref<AdminUser[]>([]);

  /** 把后端 AdminUser 映射为视图用的 SystemUser（角色名归一化为三档 + 中文标签/描述）。 */
  function mapAdminUser(record: AdminUser): SystemUser {
    const normalized = record.roleName.trim().toLowerCase();
    const role: SystemUser["role"] =
      normalized === "owner" || normalized === "admin" || normalized === "administrator"
        ? "admin"
        : normalized === "guest"
          ? "guest"
          : "user";
    const roleLabels: Record<SystemUser["role"], string> = {
      admin: "超级管理员",
      user: "标准用户",
      guest: "访客用户",
    };
    const descs: Record<SystemUser["role"], string> = {
      admin: "最高权限，拥有系统后台全部控制权限。",
      user: "可使用影视前台进行点播，无法进入管理控制台。",
      guest: "只读影视网格预览，禁止串流播放原始音视频数据。",
    };
    return {
      id: record.id,
      username: record.username,
      displayName: record.displayName,
      role,
      roleLabel: roleLabels[role],
      active: !record.isDisabled,
      desc: descs[role],
      // 媒体库授权由独立接口管理，列表态暂不展开。
      libraries: [],
      allowDownload: record.allowDownload,
      allowTranscode: record.allowTranscode,
      allowNewDeviceLogin: record.allowNewDeviceLogin,
      deviceCount: record.deviceCount,
      activeSessionCount: record.activeSessionCount,
      lastLoginAt: record.lastLoginAt,
    };
  }

  /** 从后端加载系统用户列表。 */
  async function loadUsers(): Promise<void> {
    usersLoading.value = true;
    try {
      const records = await listSystemUsers();
      userRecords.value = records;
      users.value = records.map(mapAdminUser);
    } catch {
      uiStore.showToast("加载用户列表失败，请检查网络与权限。", "error");
    } finally {
      usersLoading.value = false;
    }
  }

  function updateProfile(profile: Partial<UserProfile>) {
    if (profile.username !== undefined) {
      if (!profile.username.trim()) {
        uiStore.showToast("用户名不能为空！", "error");
        return false;
      }
      username.value = profile.username.trim();
      localStorage.setItem("fbz_auth_username", username.value);
    }
    if (profile.email !== undefined) {
      email.value = profile.email.trim();
      localStorage.setItem("fbz_auth_email", email.value);
    }
    if (profile.nickname !== undefined) {
      nickname.value = profile.nickname.trim() || username.value;
      localStorage.setItem("fbz_auth_nickname", nickname.value);
    }
    if (profile.language !== undefined) {
      language.value = profile.language;
      localStorage.setItem("fbz_pref_language", language.value);
    }
    if (profile.autoSubtitles !== undefined) {
      autoSubtitles.value = profile.autoSubtitles;
      localStorage.setItem("fbz_pref_autosub", String(autoSubtitles.value));
    }
    if (profile.audioPreference !== undefined) {
      audioPreference.value = profile.audioPreference;
      localStorage.setItem("fbz_pref_audiopref", audioPreference.value);
    }

    uiStore.showToast("个人信息与偏好设置已成功保存！", "success");
    return true;
  }

  function setLanguage(lang: string) {
    if (!lang) return;
    language.value = lang;
    localStorage.setItem("fbz_pref_language", lang);
  }

  interface LoginPayload {
    username: string;
    password: string;
    remember?: boolean;
  }

  /**
   * 真实登录：调用 fbz-api 的 `AuthenticateByName`，成功后持久化令牌与会话。
   * 失败按错误类型给文案（凭据错 / 连不上服务器）并返回 false，调用方据此停留登录页。
   */
  async function login(payload: LoginPayload): Promise<boolean> {
    if (!payload.username.trim()) {
      uiStore.showToast("请输入用户名！", "warning");
      return false;
    }
    if (!payload.password) {
      uiStore.showToast("请输入登录密码！", "warning");
      return false;
    }

    try {
      const session = await loginRequest({
        username: payload.username.trim(),
        password: payload.password,
      });

      username.value = session.username;
      userId.value = session.userId;
      isAuthenticated.value = true;
      localStorage.setItem("fbz_auth_username", session.username);
      localStorage.setItem("fbz_auth_user_id", session.userId);

      uiStore.showToast(`欢迎回来，${nickname.value || session.username}！`, "success");
      return true;
    } catch (error) {
      if (isAxiosError(error) && error.response) {
        const status = error.response.status;
        if (status === 401 || status === 403) {
          uiStore.showToast("用户名或密码错误，请重试。", "error");
        } else {
          uiStore.showToast(`登录失败（服务器返回 ${status}）。`, "error");
        }
      } else {
        uiStore.showToast("无法连接服务器，请检查网络与服务器状态。", "error");
      }
      return false;
    }
  }

  function logout() {
    logoutRequest();
    isAuthenticated.value = false;
    userId.value = "";
    localStorage.removeItem("fbz_auth_user_id");
  }

  function changePassword(currentPass: string, newPass: string) {
    if (!currentPass) {
      uiStore.showToast("请输入当前密码！", "warning");
      return false;
    }
    if (!newPass) {
      uiStore.showToast("请输入新密码！", "warning");
      return false;
    }
    if (newPass.length < 6) {
      uiStore.showToast("新密码长度不能少于 6 位！", "warning");
      return false;
    }

    uiStore.showToast("密码修改成功！请牢记您的新密码。", "success");
    return true;
  }

  // System Users Management actions（全部接 fbz-api `/api/admin/users`）。
  /** 切换用户启用/禁用：复用 policy 接口，需带上完整权限标志。 */
  async function toggleUserStatus(userId: string): Promise<void> {
    const record = userRecords.value.find((u) => u.id === userId);
    if (!record) return;
    try {
      const updated = await updateSystemUserPolicy(userId, {
        displayName: record.displayName ?? undefined,
        isDisabled: !record.isDisabled,
        allowDownload: record.allowDownload,
        allowTranscode: record.allowTranscode,
        allowNewDeviceLogin: record.allowNewDeviceLogin,
      });
      applyUserRecord(updated);
      uiStore.showToast(
        `用户【${updated.username}】状态已更新为：${updated.isDisabled ? "禁用" : "启用"}。`,
        "success",
      );
    } catch (error) {
      reportUserMutationError(error, "更新用户状态");
    }
  }

  async function deleteUser(userId: string): Promise<void> {
    const record = userRecords.value.find((u) => u.id === userId);
    if (!record) return;
    try {
      await deleteSystemUser(userId);
      userRecords.value = userRecords.value.filter((u) => u.id !== userId);
      users.value = users.value.filter((u) => u.id !== userId);
      uiStore.showToast(`用户【${record.username}】已被永久删除。`, "success");
    } catch (error) {
      reportUserMutationError(error, "删除用户");
    }
  }

  /** 创建用户。后端要求密码 ≥6 位、用户名唯一。 */
  async function addUser(payload: {
    username: string;
    password: string;
    role: SystemUser["role"];
    active: boolean;
  }): Promise<boolean> {
    try {
      const created = await createSystemUser({
        username: payload.username,
        password: payload.password,
        role: payload.role,
      });
      let record = created;
      // 若需建为禁用态，建后再补一次 policy（建用户接口默认启用）。
      if (!payload.active) {
        record = await updateSystemUserPolicy(created.id, {
          displayName: created.displayName ?? undefined,
          isDisabled: true,
          allowDownload: created.allowDownload,
          allowTranscode: created.allowTranscode,
          allowNewDeviceLogin: created.allowNewDeviceLogin,
        });
      }
      applyUserRecord(record, true);
      uiStore.showToast(`用户【${record.username}】已成功创建！`, "success");
      return true;
    } catch (error) {
      reportUserMutationError(error, "创建用户");
      return false;
    }
  }

  /**
   * 更新用户。后端 policy 接口只能改启用态与显示名等策略字段；
   * **不支持改用户名与角色**（无对应接口），这两项在编辑态由 UI 锁定。
   */
  async function updateUser(userId: string, data: { active?: boolean }): Promise<boolean> {
    const record = userRecords.value.find((u) => u.id === userId);
    if (!record) return false;
    try {
      const updated = await updateSystemUserPolicy(userId, {
        displayName: record.displayName ?? undefined,
        isDisabled: data.active === undefined ? record.isDisabled : !data.active,
        allowDownload: record.allowDownload,
        allowTranscode: record.allowTranscode,
        allowNewDeviceLogin: record.allowNewDeviceLogin,
      });
      applyUserRecord(updated);
      uiStore.showToast(`用户【${updated.username}】信息已成功保存！`, "success");
      return true;
    } catch (error) {
      reportUserMutationError(error, "保存用户");
      return false;
    }
  }

  /** 把单条后端记录写回列表（新增或就地更新）。 */
  function applyUserRecord(record: AdminUser, prepend = false): void {
    const mapped = mapAdminUser(record);
    const idx = userRecords.value.findIndex((u) => u.id === record.id);
    if (idx > -1) {
      userRecords.value[idx] = record;
      users.value[idx] = mapped;
    } else if (prepend) {
      userRecords.value.unshift(record);
      users.value.unshift(mapped);
    } else {
      userRecords.value.push(record);
      users.value.push(mapped);
    }
  }

  /** 用户增删改的统一错误文案（区分 409 守卫与其它失败）。 */
  function reportUserMutationError(error: unknown, action: string): void {
    if (isAxiosError(error) && error.response) {
      const status = error.response.status;
      if (status === 409) {
        uiStore.showToast(
          `${action}失败：操作与服务器状态冲突（如重名、删除最后管理员或自身）。`,
          "error",
        );
      } else if (status === 422) {
        uiStore.showToast(`${action}失败：输入不符合要求（密码至少 6 位）。`, "error");
      } else if (status === 401 || status === 403) {
        uiStore.showToast(`${action}失败：需要管理员权限。`, "error");
      } else {
        uiStore.showToast(`${action}失败（服务器返回 ${status}）。`, "error");
      }
    } else {
      uiStore.showToast(`${action}失败：无法连接服务器。`, "error");
    }
  }

  return {
    username,
    email,
    nickname,
    language,
    autoSubtitles,
    audioPreference,
    isAuthenticated,
    userId,
    avatarVersion,
    bumpAvatarVersion,
    users,
    usersLoading,
    login,
    logout,
    setLanguage,
    updateProfile,
    changePassword,
    loadUsers,
    toggleUserStatus,
    deleteUser,
    addUser,
    updateUser,
  };
});
