/**
 * 管理端 API service：映射 fbz-api 的媒体库与元数据刮削管理接口
 * （`src/admin/routes.rs`，全部需 server-admin 权限，经 `x-emby-token` 鉴权）。
 *
 * keyset 分页接口的主体是数组，翻页状态在响应头
 * `x-fbz-has-more` / `x-fbz-next-cursor` 返回，这里归一化为 `Paginated<T>`。
 */
import type { AxiosResponse } from "axios";

import { request } from "@/service/request.ts";
import type {
  AdminUser,
  AdminJob,
  AdminJobDetail,
  AdminJobQuery,
  CreateLibraryRequest,
  CreateUserRequest,
  LibraryListQuery,
  LibraryMetadataRefreshQueue,
  LibraryPath,
  LibraryPhoto,
  LibraryPhotoListQuery,
  LibrarySettings,
  ManagedLibrary,
  MetadataRefreshJob,
  MetadataGlobalSettings,
  MetadataProviderSettings,
  MetadataSettingsResponse,
  Paginated,
  PluginConfig,
  PluginDispatch,
  PluginDispatchQuery,
  PluginExecutionRun,
  PluginExecutionRunQuery,
  PluginHostApiCall,
  PluginListQuery,
  PluginPackageDetail,
  PluginPackageListQuery,
  PluginPackageSummary,
  PluginState,
  PluginSummary,
  ProviderProbeResult,
  ScheduledTask,
  ScheduledTaskQuery,
  ScheduledTaskRunHistory,
  ScheduledTaskRunQuery,
  ScheduledTaskRunSummary,
  ScanJob,
  UpdateUserLibraryPermissionRequest,
  UpdateLibrarySettingsRequest,
  UpdateUserPolicyRequest,
  UserLibraryPermission,
} from "@/types/admin.ts";

/** 从 keyset 列表响应的头部解出翻页状态。 */
function paginationFromHeaders<T>(response: AxiosResponse<T[]>): Paginated<T> {
  const hasMore = response.headers["x-fbz-has-more"] === "true";
  const rawCursor = response.headers["x-fbz-next-cursor"];
  const nextCursor = typeof rawCursor === "string" && rawCursor.length > 0 ? rawCursor : null;
  return { items: response.data, hasMore, nextCursor };
}

// ---- 媒体库 ----

/** 创建媒体库。 */
export async function createLibrary(payload: CreateLibraryRequest): Promise<ManagedLibrary> {
  const { data } = await request.post<ManagedLibrary>("/admin/libraries", payload);
  return data;
}

/** 列出媒体库及完整设置（keyset 分页）。 */
export async function listLibraries(
  query: LibraryListQuery = {},
): Promise<Paginated<LibrarySettings>> {
  const response = await request.get<LibrarySettings[]>("/admin/libraries", { params: query });
  return paginationFromHeaders(response);
}

/** 整体替换媒体库设置。 */
export async function updateLibrarySettings(
  libraryId: string,
  payload: UpdateLibrarySettingsRequest,
): Promise<LibrarySettings> {
  const { data } = await request.post<LibrarySettings>(
    `/admin/libraries/${encodeURIComponent(libraryId)}/settings`,
    payload,
  );
  return data;
}

/** 删除媒体库（级联清理路径与权限）。 */
export async function deleteLibrary(libraryId: string): Promise<void> {
  await request.delete(`/admin/libraries/${encodeURIComponent(libraryId)}`);
}

/** 将某媒体库加入扫描队列。 */
export async function queueLibraryScan(libraryId: string, reason?: string): Promise<ScanJob> {
  const { data } = await request.post<ScanJob>(
    `/admin/libraries/${encodeURIComponent(libraryId)}/scan`,
    { reason },
  );
  return data;
}

// ---- 媒体库路径 ----

/** 添加媒体库路径。 */
export async function addLibraryPath(libraryId: string, path: string): Promise<LibraryPath> {
  const { data } = await request.post<LibraryPath>(
    `/admin/libraries/${encodeURIComponent(libraryId)}/paths`,
    { path },
  );
  return data;
}

/** 列出媒体库已配置路径。 */
export async function listLibraryPaths(libraryId: string): Promise<LibraryPath[]> {
  const { data } = await request.get<LibraryPath[]>(
    `/admin/libraries/${encodeURIComponent(libraryId)}/paths`,
  );
  return data;
}

/** 删除媒体库单个路径。 */
export async function removeLibraryPath(libraryId: string, pathId: string): Promise<void> {
  await request.delete(
    `/admin/libraries/${encodeURIComponent(libraryId)}/paths/${encodeURIComponent(pathId)}`,
  );
}

/** 启用/禁用媒体库单个路径。 */
export async function setLibraryPathEnabled(
  libraryId: string,
  pathId: string,
  isEnabled: boolean,
): Promise<LibraryPath> {
  const { data } = await request.post<LibraryPath>(
    `/admin/libraries/${encodeURIComponent(libraryId)}/paths/${encodeURIComponent(pathId)}/settings`,
    { isEnabled },
  );
  return data;
}

// ---- 家庭库图片时间线 ----

/** 列出家庭库图片时间线（按拍摄时间倒序，keyset 分页）。 */
export async function listLibraryPhotos(
  libraryId: string,
  query: LibraryPhotoListQuery = {},
): Promise<Paginated<LibraryPhoto>> {
  const response = await request.get<LibraryPhoto[]>(
    `/admin/libraries/${encodeURIComponent(libraryId)}/photos`,
    { params: query },
  );
  return paginationFromHeaders(response);
}

/**
 * 取图片缩略图为 blob object URL。缩略图端点需鉴权（`x-emby-token`），
 * 浏览器 `<img>` 不会自动带该头，故走 axios（带拦截器）取 blob 再转 URL。
 * 调用方在不再展示时应 `URL.revokeObjectURL` 释放。尚未生成（404）时抛错，调用方降级占位。
 */
export async function fetchPhotoThumbnail(itemId: string): Promise<string> {
  const response = await request.get<Blob>(
    `/admin/media-items/${encodeURIComponent(itemId)}/thumbnail`,
    { responseType: "blob" },
  );
  return URL.createObjectURL(response.data);
}

// ---- 元数据刮削设置 ----

/** 读取元数据全局默认 + 各 provider 设置（key 仅返回 hasKey，不回显明文）。 */
export async function getMetadataSettings(): Promise<MetadataSettingsResponse> {
  const { data } = await request.get<MetadataSettingsResponse>("/admin/metadata/settings");
  return data;
}

/** 保存元数据全局默认设置。 */
export async function updateMetadataSettings(
  payload: MetadataGlobalSettings,
): Promise<MetadataSettingsResponse> {
  const { data } = await request.post<MetadataSettingsResponse>(
    "/admin/metadata/settings",
    payload,
  );
  return data;
}

/** 保存单个 provider 覆盖设置。 */
export async function updateMetadataProviderSettings(
  providerId: string,
  payload: Partial<Omit<MetadataProviderSettings, "providerId" | "hasKey">>,
): Promise<MetadataProviderSettings> {
  const { data } = await request.post<MetadataProviderSettings>(
    `/admin/metadata/providers/${encodeURIComponent(providerId)}`,
    payload,
  );
  return data;
}

/** 设置 provider 密钥，后端仅返回掩码与计数。 */
export async function setMetadataProviderKey(providerId: string, key: string): Promise<void> {
  await request.post(`/admin/metadata/providers/${encodeURIComponent(providerId)}/key`, { key });
}

/** 对某 provider 做受控连通性/鉴权探测（不写库）。 */
export async function testMetadataProvider(providerId: string): Promise<ProviderProbeResult> {
  const { data } = await request.post<ProviderProbeResult>(
    `/admin/metadata/providers/${encodeURIComponent(providerId)}/test`,
  );
  return data;
}

/** 将单个媒体条目加入元数据刷新队列。 */
export async function queueItemMetadataRefresh(
  itemId: string,
  reason?: string,
): Promise<MetadataRefreshJob> {
  const { data } = await request.post<MetadataRefreshJob>(
    `/admin/media-items/${encodeURIComponent(itemId)}/metadata/refresh`,
    { reason },
  );
  return data;
}

/** 将媒体库内条目批量加入元数据刷新队列。 */
export async function queueLibraryMetadataRefresh(
  libraryId: string,
  options: { reason?: string; limit?: number } = {},
): Promise<LibraryMetadataRefreshQueue> {
  const { data } = await request.post<LibraryMetadataRefreshQueue>(
    `/admin/libraries/${encodeURIComponent(libraryId)}/metadata/refresh`,
    options,
  );
  return data;
}

// ---- 系统用户 ----

/** 列出系统用户（keyset 分页，本应用一次性取前若干条）。 */
export async function listSystemUsers(): Promise<AdminUser[]> {
  const { data } = await request.get<AdminUser[]>("/admin/users", { params: { limit: 200 } });
  return data;
}

/** 创建系统用户（用户名唯一，密码 ≥6 位，role 取 admin/user/guest）。 */
export async function createSystemUser(payload: CreateUserRequest): Promise<AdminUser> {
  const { data } = await request.post<AdminUser>("/admin/users", payload);
  return data;
}

/** 删除系统用户（不能删自己与最后一个管理员，后端会返回 409）。 */
export async function deleteSystemUser(userId: string): Promise<void> {
  await request.delete(`/admin/users/${encodeURIComponent(userId)}`);
}

/** 替换用户全局策略（启用态/显示名/下载转码权限）。 */
export async function updateSystemUserPolicy(
  userId: string,
  payload: UpdateUserPolicyRequest,
): Promise<AdminUser> {
  const { data } = await request.put<AdminUser>(
    `/admin/users/${encodeURIComponent(userId)}/policy`,
    payload,
  );
  return data;
}

/** 列出某用户对所有媒体库的显式/有效权限。 */
export async function listUserLibraryPermissions(userId: string): Promise<UserLibraryPermission[]> {
  const { data } = await request.get<UserLibraryPermission[]>(
    `/admin/users/${encodeURIComponent(userId)}/libraries`,
    { params: { limit: 500 } },
  );
  return data;
}

/** 替换某用户对单个媒体库的权限。 */
export async function updateUserLibraryPermission(
  userId: string,
  libraryId: string,
  payload: UpdateUserLibraryPermissionRequest,
): Promise<UserLibraryPermission> {
  const { data } = await request.put<UserLibraryPermission>(
    `/admin/users/${encodeURIComponent(userId)}/libraries/${encodeURIComponent(libraryId)}/permissions`,
    payload,
  );
  return data;
}

// ---- Jobs / scheduled tasks ----

/** 列出后台 jobs。 */
export async function listAdminJobs(query: AdminJobQuery = {}): Promise<Paginated<AdminJob>> {
  const response = await request.get<AdminJob[]>("/admin/jobs", { params: query });
  return paginationFromHeaders(response);
}

/** 读取 job 详情、最近运行与事件。 */
export async function getAdminJobDetail(jobId: string): Promise<AdminJobDetail> {
  const { data } = await request.get<AdminJobDetail>(`/admin/jobs/${encodeURIComponent(jobId)}`);
  return data;
}

/** 手动触发支持的后台 job。 */
export async function runAdminJob(jobId: string): Promise<AdminJob> {
  const { data } = await request.post<AdminJob>(`/admin/jobs/${encodeURIComponent(jobId)}/run`);
  return data;
}

/** 列出计划任务。 */
export async function listScheduledTasks(
  query: ScheduledTaskQuery = {},
): Promise<Paginated<ScheduledTask>> {
  const response = await request.get<ScheduledTask[]>("/admin/scheduled-tasks", { params: query });
  return paginationFromHeaders(response);
}

/** 列出计划任务运行历史。 */
export async function listScheduledTaskRuns(
  taskKey: string,
  query: ScheduledTaskRunQuery = {},
): Promise<Paginated<ScheduledTaskRunHistory>> {
  const response = await request.get<ScheduledTaskRunHistory[]>(
    `/admin/scheduled-tasks/${encodeURIComponent(taskKey)}/runs`,
    { params: query },
  );
  return paginationFromHeaders(response);
}

/** 手动运行一个计划任务。 */
export async function runScheduledTask(taskKey: string): Promise<ScheduledTaskRunSummary> {
  const { data } = await request.post<ScheduledTaskRunSummary>(
    `/admin/scheduled-tasks/${encodeURIComponent(taskKey)}/run`,
  );
  return data;
}

// ---- Plugins ----

/** 列出已安装/已注册插件状态。 */
export async function listPlugins(query: PluginListQuery = {}): Promise<Paginated<PluginSummary>> {
  const response = await request.get<PluginSummary[]>("/admin/plugins", { params: query });
  return paginationFromHeaders(response);
}

/** 列出插件包。 */
export async function listPluginPackages(
  query: PluginPackageListQuery = {},
): Promise<Paginated<PluginPackageSummary>> {
  const response = await request.get<PluginPackageSummary[]>("/admin/plugins/packages", {
    params: query,
  });
  return paginationFromHeaders(response);
}

/** 读取插件包详情。 */
export async function getPluginPackageDetail(packageId: string): Promise<PluginPackageDetail> {
  const { data } = await request.get<PluginPackageDetail>(
    `/admin/plugins/packages/${encodeURIComponent(packageId)}`,
  );
  return data;
}

export async function approvePluginPackage(packageId: string): Promise<PluginState> {
  const { data } = await request.post<PluginState>(
    `/admin/plugins/packages/${encodeURIComponent(packageId)}/approve`,
  );
  return data;
}

export async function rejectPluginPackage(packageId: string): Promise<PluginState> {
  const { data } = await request.post<PluginState>(
    `/admin/plugins/packages/${encodeURIComponent(packageId)}/reject`,
  );
  return data;
}

export async function activatePluginPackage(packageId: string): Promise<PluginState> {
  const { data } = await request.post<PluginState>(
    `/admin/plugins/packages/${encodeURIComponent(packageId)}/activate`,
  );
  return data;
}

export async function setPluginEnabled(pluginId: string, enabled: boolean): Promise<PluginState> {
  const action = enabled ? "enable" : "disable";
  const { data } = await request.post<PluginState>(
    `/admin/plugins/${encodeURIComponent(pluginId)}/${action}`,
  );
  return data;
}

export async function getPluginConfig(pluginId: string): Promise<PluginConfig> {
  const { data } = await request.get<PluginConfig>(
    `/admin/plugins/${encodeURIComponent(pluginId)}/config`,
  );
  return data;
}

export async function updatePluginConfig(
  pluginId: string,
  values: Record<string, unknown>,
): Promise<PluginConfig> {
  const { data } = await request.put<PluginConfig>(
    `/admin/plugins/${encodeURIComponent(pluginId)}/config`,
    { values },
  );
  return data;
}

/** 列出插件 dispatch 审计队列。 */
export async function listPluginDispatches(
  query: PluginDispatchQuery = {},
): Promise<Paginated<PluginDispatch>> {
  const response = await request.get<PluginDispatch[]>("/admin/plugin-dispatches", {
    params: query,
  });
  return paginationFromHeaders(response);
}

/** 列出某 dispatch 的插件运行记录。 */
export async function listPluginExecutionRuns(
  dispatchId: string,
  query: PluginExecutionRunQuery = {},
): Promise<Paginated<PluginExecutionRun>> {
  const response = await request.get<PluginExecutionRun[]>(
    `/admin/plugin-dispatches/${encodeURIComponent(dispatchId)}/runs`,
    { params: query },
  );
  return paginationFromHeaders(response);
}

/** 重放一个失败/已存在的插件 dispatch。 */
export async function replayPluginDispatch(dispatchId: string): Promise<PluginDispatch> {
  const { data } = await request.post<PluginDispatch>(
    `/admin/plugin-dispatches/${encodeURIComponent(dispatchId)}/replay`,
  );
  return data;
}

/** 列出插件调用 Host API 的审计记录。 */
export async function listPluginHostApiCalls(
  params: {
    pluginId?: string;
    executionRunId?: string;
    statusCode?: number;
    cursor?: string;
    limit?: number;
  } = {},
): Promise<Paginated<PluginHostApiCall>> {
  const response = await request.get<PluginHostApiCall[]>("/admin/plugin-host-api-calls", {
    params,
  });
  return paginationFromHeaders(response);
}
