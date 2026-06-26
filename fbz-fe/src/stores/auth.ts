import { ref } from "vue";
import { defineStore } from "pinia";
import { useUiStore } from "@/stores/ui.ts";

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
  role: "admin" | "user" | "guest";
  roleLabel: string;
  active: boolean;
  desc: string;
  libraries: string[];
}

export const useAuthStore = defineStore("auth", () => {
  const uiStore = useUiStore();

  const username = ref<string>(localStorage.getItem("fbz_auth_username") ?? "admin");
  const email = ref<string>(localStorage.getItem("fbz_auth_email") ?? "admin@fbz.com");
  const nickname = ref<string>(localStorage.getItem("fbz_auth_nickname") ?? "Admin");

  const language = ref<string>(localStorage.getItem("fbz_pref_language") ?? "zh-CN");
  const autoSubtitles = ref<boolean>(localStorage.getItem("fbz_pref_autosub") !== "false");
  const audioPreference = ref<string>(localStorage.getItem("fbz_pref_audiopref") ?? "zh");

  // 登录态（设计阶段为本地 mock，接后端后替换为真实会话）
  const serverAddress = ref<string>(localStorage.getItem("fbz_server_address") ?? "");
  const isAuthenticated = ref<boolean>(localStorage.getItem("fbz_authenticated") === "true");

  // System Users List
  const savedUsers = localStorage.getItem("fbz_system_users");
  const defaultUsers: SystemUser[] = [
    {
      id: "u1",
      username: "admin",
      role: "admin",
      roleLabel: "超级管理员",
      active: true,
      desc: "最高权限，拥有系统后台全部控制权限。",
      libraries: ["lib-1", "lib-2"],
    },
    {
      id: "u2",
      username: "Alan",
      role: "user",
      roleLabel: "标准用户",
      active: true,
      desc: "可使用影视前台进行点播，无法进入管理控制台。",
      libraries: ["lib-1"],
    },
    {
      id: "u3",
      username: "Guest",
      role: "guest",
      roleLabel: "访客用户",
      active: false,
      desc: "只读影视网格预览，禁止串流播放原始音视频数据。",
      libraries: ["lib-1"],
    },
  ];
  const users = ref<SystemUser[]>(savedUsers ? JSON.parse(savedUsers) : defaultUsers);

  function saveUsersToStorage() {
    localStorage.setItem("fbz_system_users", JSON.stringify(users.value));
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
    serverAddress?: string;
    remember?: boolean;
  }

  function login(payload: LoginPayload) {
    if (!payload.username.trim()) {
      uiStore.showToast("请输入用户名！", "warning");
      return false;
    }
    if (!payload.password) {
      uiStore.showToast("请输入登录密码！", "warning");
      return false;
    }

    username.value = payload.username.trim();
    if (payload.serverAddress !== undefined) {
      serverAddress.value = payload.serverAddress.trim();
      localStorage.setItem("fbz_server_address", serverAddress.value);
    }

    isAuthenticated.value = true;
    localStorage.setItem("fbz_auth_username", username.value);
    if (payload.remember) {
      localStorage.setItem("fbz_authenticated", "true");
    } else {
      localStorage.removeItem("fbz_authenticated");
    }

    uiStore.showToast(`欢迎回来，${nickname.value || username.value}！`, "success");
    return true;
  }

  function logout() {
    isAuthenticated.value = false;
    localStorage.removeItem("fbz_authenticated");
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

  // System Users Management actions
  function toggleUserStatus(userId: string) {
    const user = users.value.find((u) => u.id === userId);
    if (user) {
      user.active = !user.active;
      saveUsersToStorage();
      uiStore.showToast(
        `用户【${user.username}】状态已更新为：${user.active ? "启用" : "禁用"}。`,
        "success",
      );
    }
  }

  function deleteUser(userId: string) {
    const user = users.value.find((u) => u.id === userId);
    if (!user) return;
    if (user.username === "admin") {
      uiStore.showToast("不可删除默认超级管理员账号！", "error");
      return;
    }
    const idx = users.value.findIndex((u) => u.id === userId);
    if (idx > -1) {
      users.value.splice(idx, 1);
      saveUsersToStorage();
      uiStore.showToast(`用户【${user.username}】已被永久删除。`, "success");
    }
  }

  function addUser(user: Omit<SystemUser, "id" | "roleLabel" | "desc">) {
    const roleLabels = {
      admin: "超级管理员",
      user: "标准用户",
      guest: "访客用户",
    };
    const descs = {
      admin: "最高权限，拥有系统后台全部控制权限。",
      user: "可使用影视前台进行点播，无法进入管理控制台。",
      guest: "只读影视网格预览，禁止串流播放原始音视频数据。",
    };

    const newUser: SystemUser = {
      id: `u-${Date.now()}`,
      username: user.username,
      role: user.role,
      roleLabel: roleLabels[user.role] || "标准用户",
      active: user.active,
      desc: descs[user.role] || descs.user,
      libraries: user.libraries,
    };

    users.value.push(newUser);
    saveUsersToStorage();
    uiStore.showToast(`用户【${user.username}】已成功创建！`, "success");
  }

  function updateUser(
    userId: string,
    data: Partial<Omit<SystemUser, "id" | "roleLabel" | "desc">>,
  ) {
    const user = users.value.find((u) => u.id === userId);
    if (!user) return;

    if (data.username !== undefined) user.username = data.username;
    if (data.role !== undefined) {
      const roleLabels = {
        admin: "超级管理员",
        user: "标准用户",
        guest: "访客用户",
      };
      const descs = {
        admin: "最高权限，拥有系统后台全部控制权限。",
        user: "可使用影视前台进行点播，无法进入管理控制台。",
        guest: "只读影视网格预览，禁止串流播放原始音视频数据。",
      };
      user.role = data.role;
      user.roleLabel = roleLabels[data.role] || "标准用户";
      user.desc = descs[data.role] || descs.user;
    }
    if (data.active !== undefined) user.active = data.active;
    if (data.libraries !== undefined) user.libraries = data.libraries;

    saveUsersToStorage();
    uiStore.showToast(`用户【${user.username}】信息已成功保存！`, "success");
  }

  return {
    username,
    email,
    nickname,
    language,
    autoSubtitles,
    audioPreference,
    serverAddress,
    isAuthenticated,
    users,
    login,
    logout,
    setLanguage,
    updateProfile,
    changePassword,
    toggleUserStatus,
    deleteUser,
    addUser,
    updateUser,
  };
});
