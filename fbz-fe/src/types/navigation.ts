/** 导航 BFF (`GET /api/navigation`) 与登录 (`/Users/AuthenticateByName`) 的响应类型。 */

import type { ContinueItem, FeaturedItem } from "@/types/media.ts";

/** 登录成功后端返回的会话信息（PascalCase，对齐 Emby `AuthenticationResultDto`）。 */
export interface AuthenticationResult {
  AccessToken: string;
  ServerId: string;
  User: {
    Id: string;
    Name: string;
  };
  SessionInfo?: {
    Id: string;
    UserId: string;
    UserName: string;
  };
}

/** 登录入参。 */
export interface LoginPayload {
  username: string;
  password: string;
}

/** 登录归一化结果（前端内部用 camelCase）。 */
export interface AuthSession {
  accessToken: string;
  userId: string;
  username: string;
  serverId: string;
}

/* ---------- 导航 BFF（camelCase，对齐后端 `navigation/dto.rs`） ---------- */

/** 当前登录用户的精简档案。 */
export interface NavigationUser {
  id: string;
  name: string;
  isAdmin: boolean;
}

/** 一个媒体库视图。 */
export interface NavigationLibrary {
  id: string;
  name: string;
  /** best-effort 前端展示类型（movie/series/music/mixed/…）。 */
  kind: string;
  /** 后端规范库类型（Emby CollectionType 词汇）。 */
  collectionType: string;
  count: number;
}

/** 首页一行内容（后端原始形状，含分区元信息）。 */
export interface NavigationSection {
  key: string;
  title: string;
  layout: "poster" | "wide";
  to?: string;
  items: NavigationMediaItem[];
}

/** 媒体条目（后端原始形状，poster 为服务器根路径，需经 mediaImageUrl 拼绝对地址）。 */
export interface NavigationMediaItem {
  id: string;
  libraryId?: string;
  title: string;
  meta: string;
  detailType?: "movie" | "tv";
  poster?: string;
  year?: number;
  rating?: number;
  progress?: number;
}

/** 首页 hero 主打项（后端原始形状）。 */
export interface NavigationFeatured {
  id: string;
  title: string;
  meta: string[];
  tags: string[];
  overview: string;
  backdrop?: string;
  thumb?: string;
  detailType?: "movie" | "tv";
}

/** `GET /api/navigation` 的完整响应。 */
export interface NavigationResponse {
  user: NavigationUser;
  libraries: NavigationLibrary[];
  sections: NavigationSection[];
  featured: NavigationFeatured[];
}

/** 前端消费态：分区已映射为组件直接可用的 ContinueItem 行，featured 已拼好图片地址。 */
export interface HomeData {
  featured: FeaturedItem[];
  rows: HomeRow[];
}

/** 首页一行（映射后，items 直接喂给 MediaRow）。 */
export interface HomeRow {
  key: string;
  title: string;
  layout: "poster" | "wide";
  to?: string;
  items: ContinueItem[];
}
