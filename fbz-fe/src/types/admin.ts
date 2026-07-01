/**
 * Admin API 类型定义，精确映射 fbz-api 的管理端 DTO（serde `rename_all = "camelCase"`）。
 * 对应后端 `src/admin/routes.rs` 的媒体库与元数据刮削管理接口。
 */

/**
 * 媒体库类型，逐字对应后端 `library_type` 校验 allowlist
 * （单一事实源：fbz-api `src/media_types.rs` 的 `LibraryType`，与 Emby CollectionType 对齐）。
 * 改动时务必与后端同步，否则建库会被后端 422 拒绝。
 */
export type AdminLibraryType = "movies" | "tvshows" | "music" | "homevideos" | "mixed" | "livetv";

/** 建库/筛选下拉用的库类型选项（label 面向用户，value 即后端契约值）。 */
export const LIBRARY_TYPE_OPTIONS: { label: string; value: AdminLibraryType }[] = [
  { label: "电影 (Movies)", value: "movies" },
  { label: "电视剧 (TV Shows)", value: "tvshows" },
  { label: "音乐 (Music)", value: "music" },
  { label: "家庭视频 (Home Videos)", value: "homevideos" },
  { label: "混合影视 (Mixed)", value: "mixed" },
  { label: "直播 (Live TV)", value: "livetv" },
];

/** 创建媒体库返回的精简记录（`ManagedLibraryDto`）。 */
export interface ManagedLibrary {
  id: string;
  name: string;
  libraryType: string;
}

/** 媒体库完整设置（`LibrarySettingsDto`），列表/读回用。 */
export interface LibrarySettings {
  id: string;
  name: string;
  libraryType: string;
  isHidden: boolean;
  preferredMetadataLanguage: string | null;
  preferredMetadataCountry: string | null;
  preferredImageLanguage: string | null;
  preferredImagePreferOriginal: boolean | null;
  preferredImageFallbackLanguages: string[];
}

/** 媒体库路径（`LibraryPathDto`）。 */
export interface LibraryPath {
  id: string;
  libraryId: string;
  path: string;
  isEnabled: boolean;
}

/** 创建媒体库请求体（`CreateLibraryRequestDto`）。 */
export interface CreateLibraryRequest {
  name: string;
  libraryType: AdminLibraryType;
  paths?: string[];
  preferredMetadataLanguage?: string | null;
  preferredMetadataCountry?: string | null;
  preferredImageLanguage?: string | null;
  preferredImagePreferOriginal?: boolean | null;
  preferredImageFallbackLanguages?: string[];
}

/** 整体替换媒体库设置请求体（`UpdateLibrarySettingsRequestDto`）。 */
export interface UpdateLibrarySettingsRequest {
  isHidden: boolean;
  preferredMetadataLanguage?: string | null;
  preferredMetadataCountry?: string | null;
  preferredImageLanguage?: string | null;
  preferredImagePreferOriginal?: boolean | null;
  preferredImageFallbackLanguages?: string[];
}

/** 列表查询参数，翻页通过响应头 `x-fbz-next-cursor` 返回。 */
export interface LibraryListQuery {
  libraryType?: AdminLibraryType;
  isHidden?: boolean;
  cursor?: string;
  limit?: number;
}

/** keyset 分页结果：数组主体 + 头部翻页状态。 */
export interface Paginated<T> {
  items: T[];
  hasMore: boolean;
  nextCursor: string | null;
}

/**
 * 家庭库图片时间线一行，对应后端 `LibraryPhotoDto`
 * （`GET /api/admin/libraries/{id}/photos`）。EXIF 字段尽力而为，缺失即 null。
 */
export interface LibraryPhoto {
  id: string;
  title: string;
  width: number | null;
  height: number | null;
  capturedAt: string | null;
  cameraMake: string | null;
  cameraModel: string | null;
  lensModel: string | null;
  orientation: number | null;
  iso: number | null;
  fNumber: number | null;
  exposureTime: string | null;
  focalLength: number | null;
  gpsLatitude: number | null;
  gpsLongitude: number | null;
  gpsAltitude: number | null;
  hasThumbnail: boolean;
}

/** 图片时间线查询参数（keyset 翻页）。 */
export interface LibraryPhotoListQuery {
  cursor?: string;
  limit?: number;
}

/** `POST /api/admin/libraries/{id}/scan` 返回的扫描任务。 */
export interface ScanJob {
  id: string;
  status: string;
  queueName: string;
  jobType: string;
}

/** `POST /api/admin/media-items/{id}/metadata/refresh` 返回的元数据刷新任务。 */
export interface MetadataRefreshJob {
  id: string;
  status: string;
  queueName: string;
  jobType: string;
  itemId: string;
}

/** `POST /api/admin/libraries/{id}/metadata/refresh` 返回的批量刷新入队摘要。 */
export interface LibraryMetadataRefreshQueue {
  libraryId: string;
  queuedJobs: number;
}

/** 元数据全局默认设置（`MetadataGlobalSettingsDto`）。 */
export interface MetadataGlobalSettings {
  providerOrder: string[];
  defaultLanguage: string | null;
  defaultCountry: string | null;
  imageLanguage: string | null;
  imagePreferOriginal: boolean;
  imageFallbackLanguages: string[];
}

/** 单 provider 设置（`MetadataProviderSettingsDto`），key 永不回显明文。 */
export interface MetadataProviderSettings {
  providerId: string;
  enabled: boolean;
  apiBaseUrl: string | null;
  imageBaseUrl: string | null;
  proxyMode: string;
  proxyUrl: string | null;
  language: string | null;
  country: string | null;
  imageLanguage: string | null;
  imagePreferOriginal: boolean | null;
  hasKey: boolean;
}

/** `GET /api/admin/metadata/settings` 响应（`MetadataSettingsResponseDto`）。 */
export interface MetadataSettingsResponse {
  global: MetadataGlobalSettings;
  providers: MetadataProviderSettings[];
}

/** provider 连通性探测结果（`ProviderProbeResult`）。 */
export interface ProviderProbeResult {
  provider: string;
  ok: boolean;
  message: string;
}

/** 系统用户角色（前端三档，映射后端角色名）。 */
export type AdminUserRole = "admin" | "user" | "guest";

/** 系统用户记录（`AdminUserDto`），用户管理列表用。 */
export interface AdminUser {
  id: string;
  username: string;
  displayName: string | null;
  roleName: string;
  isDisabled: boolean;
  allowDownload: boolean;
  allowTranscode: boolean;
  allowNewDeviceLogin: boolean;
  hasPassword: boolean;
  deviceCount: number;
  activeSessionCount: number;
  lastLoginAt: string | null;
  createdAt: string;
  updatedAt: string;
}

/** `POST /api/admin/users` 入参。 */
export interface CreateUserRequest {
  username: string;
  password: string;
  role: AdminUserRole;
  displayName?: string;
  allowDownload?: boolean;
  allowTranscode?: boolean;
  allowNewDeviceLogin?: boolean;
}

/** `PUT /api/admin/users/{id}/policy` 入参。 */
export interface UpdateUserPolicyRequest {
  displayName?: string;
  isDisabled: boolean;
  allowDownload: boolean;
  allowTranscode: boolean;
  allowNewDeviceLogin: boolean;
}

export interface UserLibraryPermission {
  libraryId: string;
  libraryName: string;
  libraryType: string;
  permissionConfigured: boolean;
  canView: boolean;
  canDownload: boolean;
  canTranscode: boolean;
  effectiveCanView: boolean;
  effectiveCanDownload: boolean;
  effectiveCanTranscode: boolean;
  permissionUpdatedAt: string | null;
}

export interface UpdateUserLibraryPermissionRequest {
  canView: boolean;
  canDownload: boolean;
  canTranscode: boolean;
}

export interface AdminJob {
  id: string;
  jobType: string;
  status: string;
  queueName: string;
  priority: number;
  payload: unknown;
  dedupeKey: string | null;
  runAt: string;
  lockedBy: string | null;
  lockedUntil: string | null;
  lockActive: boolean;
  attempts: number;
  maxAttempts: number;
  lastError: string | null;
  createdAt: string;
  updatedAt: string;
  finishedAt: string | null;
}

export interface AdminJobQuery {
  status?: string;
  jobType?: string;
  queueName?: string;
  cursor?: string;
  limit?: number;
}

export interface AdminJobRun {
  id: number;
  workerId: string;
  status: string;
  startedAt: string;
  finishedAt: string | null;
  durationMs: number;
  errorMessage: string | null;
  metrics: unknown;
}

export interface AdminJobEvent {
  id: number;
  runId: number | null;
  eventType: string;
  eventLevel: string;
  message: string | null;
  payload: unknown;
  createdAt: string;
}

export interface AdminJobDetail {
  job: AdminJob;
  runs: AdminJobRun[];
  events: AdminJobEvent[];
}

export interface ScheduledTask {
  id: string;
  taskKey: string;
  taskType: string;
  ownerType: string;
  ownerId: string | null;
  enabled: boolean;
  scheduleKind: string;
  scheduleValue: string;
  nextRunAt: string | null;
  lastRunAt: string | null;
  timeoutSeconds: number;
  maxConcurrency: number;
  activeRunCount: number;
  lastRunId: string | null;
  failureCount: number;
  lastError: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ScheduledTaskQuery {
  taskType?: string;
  ownerType?: string;
  enabled?: boolean;
  cursor?: string;
  limit?: number;
}

export interface ScheduledTaskRunSummary {
  taskKey: string;
  taskType: string;
  queuedJobs: number;
}

export interface ScheduledTaskRunHistory {
  id: string;
  taskKey: string;
  triggerType: string;
  workerId: string;
  status: string;
  leaseExpiresAt: string;
  leaseActive: boolean;
  queuedJobs: number | null;
  errorMessage: string | null;
  startedAt: string;
  finishedAt: string | null;
  durationMs: number;
  createdAt: string;
  updatedAt: string;
}

export interface ScheduledTaskRunQuery {
  status?: string;
  cursor?: string;
  limit?: number;
}

export interface PluginSummary {
  pluginId: string;
  packageId: string | null;
  packageVersion: string | null;
  packageStatus: string | null;
  approvalStatus: string;
  enabled: boolean;
  name: string | null;
  runtime: string | null;
}

export interface PluginListQuery {
  approvalStatus?: string;
  enabled?: boolean;
  runtime?: string;
  cursor?: string;
  limit?: number;
}

export interface PluginState {
  pluginId: string;
  packageId: string | null;
  packageVersion: string | null;
  packageStatus: string | null;
  approvalStatus: string;
  enabled: boolean;
}

export interface PluginPackageSummary {
  packageId: string;
  pluginId: string;
  packageVersion: string;
  apiVersion: string;
  runtime: string;
  name: string;
  packageStatus: string;
  signaturePresent: boolean;
  approvalStatus: string | null;
  enabled: boolean | null;
  active: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface PluginPackageListQuery {
  pluginId?: string;
  packageStatus?: string;
  runtime?: string;
  cursor?: string;
  limit?: number;
}

export interface PluginPermission {
  permissionKey: string;
  permissionScope: string | null;
  reason: string | null;
}

export interface PluginHook {
  eventKey: string;
  handler: string;
  priority: number;
  enabled: boolean;
}

export interface PluginMenuItem {
  itemKey: string;
  label: string;
  path: string;
  parentKey: string | null;
  requiredPermission: string | null;
  weight: number;
  enabled: boolean;
}

export interface PluginScheduleDefinition {
  taskKey: string;
  scheduleKind: string;
  scheduleValue: string;
  handler: string;
  enabledByDefault: boolean;
  timeoutSeconds: number;
}

export interface PluginPackageDetail {
  packageId: string;
  pluginId: string;
  packageVersion: string;
  apiVersion: string;
  runtime: string;
  name: string;
  description: string | null;
  entrypoint: string;
  packagePath: string;
  packageStatus: string;
  signaturePresent: boolean;
  approvalStatus: string | null;
  enabled: boolean | null;
  permissions: PluginPermission[];
  hooks: PluginHook[];
  menu: PluginMenuItem[];
  schedules: PluginScheduleDefinition[];
}

export interface PluginConfigOption {
  value: string;
  label: string;
}

export interface PluginConfigField {
  key: string;
  label: string;
  type: string;
  required: boolean;
  helpText: string | null;
  options: PluginConfigOption[];
}

export interface PluginConfig {
  pluginId: string;
  packageId: string;
  pluginName: string;
  schema: PluginConfigField[];
  values: Record<string, unknown>;
}

export interface PluginDispatch {
  id: string;
  pluginId: string | null;
  packageId: string | null;
  hookId: string | null;
  handler: string | null;
  hookEvent: string | null;
  aggregateType: string;
  aggregateId: string;
  payload: unknown;
  status: string;
  attempts: number;
  maxAttempts: number;
  availableAt: string;
  lockedUntil: string | null;
  lastError: string | null;
  createdAt: string;
  deliveredAt: string | null;
}

export interface PluginDispatchQuery {
  status?: string;
  cursor?: string;
  limit?: number;
}

export interface PluginExecutionRun {
  id: string;
  dispatchId: string;
  outboxEventId: number | null;
  attempt: number;
  pluginId: string;
  packageId: string;
  hookId: number | null;
  handler: string;
  eventKey: string;
  runtime: string;
  entrypoint: string;
  status: string;
  requestPayload: unknown;
  responseStatus: number | null;
  responseBody: string | null;
  errorMessage: string | null;
  startedAt: string;
  finishedAt: string | null;
  durationMs: number | null;
}

export interface PluginExecutionRunQuery {
  status?: string;
  cursor?: string;
  limit?: number;
}

export interface PluginHostApiCall {
  id: string;
  pluginId: string;
  packageId: string;
  hostTokenId: string | null;
  executionRunId: string | null;
  method: string;
  path: string;
  requiredPermission: string | null;
  statusCode: number;
  errorCode: string | null;
  errorMessage: string | null;
  startedAt: string;
  finishedAt: string;
  durationMs: number;
}
