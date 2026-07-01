//! 首页/导航 BFF 的对外 DTO。
//!
//! 与 Emby compat 层的 `PascalCase` DTO 不同，这里统一 `camelCase`，直接对齐前端
//! (`fbz-fe/src/types/media.ts`) 的 `MediaItem` / `FeaturedItem` / `MediaLibrary`，
//! 让前端把 `tmdb.ts` 的占位函数原地换成一次 `GET /api/navigation` 即可，页面消费方不变。

use serde::Serialize;

/// `GET /api/navigation` 的聚合响应：一次请求拿齐首屏所需的全部数据。
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigationDto {
    /// 当前登录用户的精简档案（导航栏头像/菜单用）。
    pub user: NavigationUserDto,
    /// 当前用户可见的媒体库列表（导航栏下拉/抽屉菜单 + 媒体库总览页用）。
    pub libraries: Vec<NavigationLibraryDto>,
    /// 首页内容行（继续观看 / 最新入库 / …），顺序即渲染顺序。
    pub sections: Vec<NavigationSectionDto>,
    /// 首页 hero 轮播主打项（取自最新入库的前几条）。
    pub featured: Vec<FeaturedItemDto>,
}

/// 当前登录用户的精简档案。
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigationUserDto {
    /// 对外用户标识（Emby `public_id`），鉴权后续请求时回传。
    pub id: String,
    /// 登录名。
    pub name: String,
    /// 是否具备服务器管理权限（前端据此显隐管理后台入口）。
    pub is_admin: bool,
}

/// 一个媒体库视图（对齐前端 `MediaLibrary`）。
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigationLibraryDto {
    /// 库的对外标识（`public_id`），用于 `/library/:id` 路由与后续条目查询。
    pub id: String,
    /// 库名称。
    pub name: String,
    /// 前端展示用的库类型（best-effort：`movie`/`series`/`music`/`mixed` …）。
    ///
    /// 注意：前端的 `anime`/`documentary` 是用户语义，后端无法区分（它们本质是
    /// `tvshows`/`movies` 库），此处只给出可从 `library_type` 推断的类型。
    pub kind: String,
    /// 后端规范库类型（Emby `CollectionType` 词汇，如 `tvshows`），保留以便前端需要时精确判断。
    pub collection_type: String,
    /// 该库的条目数（近似值，见 service 层说明）。
    pub count: u32,
}

/// 首页一行内容。
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigationSectionDto {
    /// 稳定键（`continue` / `latest` / …），前端据此选布局，不随文案变化。
    pub key: String,
    /// 行标题（已本地化文案）。
    pub title: String,
    /// 行布局：`wide`（继续观看，宽幅带进度）或 `poster`（海报竖图）。
    pub layout: String,
    /// 点击「查看全部」跳转的前端路由；为空表示无总览页。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// 行内条目。
    pub items: Vec<MediaItemDto>,
}

/// 媒体条目的统一展示模型（对齐前端 `MediaItem` / `ContinueItem`）。
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItemDto {
    /// 条目对外标识（`public_id`）。
    pub id: String,
    /// 归属库标识；继续观看等跨库行可能为空，前端按 `detailType` 兜底。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library_id: Option<String>,
    /// 标题。
    pub title: String,
    /// 副标题元信息（如 "2025"），前端直接展示。
    pub meta: String,
    /// 详情路由类型：`movie` → `/movie/:id`，`tv` → `/tv/:id`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail_type: Option<String>,
    /// 海报地址（服务器根路径，如 `/Items/{id}/Images/Primary`）；为空时前端渲染占位块。
    ///
    /// 不带鉴权参数：前端 `request.ts` 拼接时会按需补 `api_key`（图片端点接受查询串 token）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
    /// 发行年份。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    /// 评分 0–10。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
    /// 观看进度百分比 0–100；仅「继续观看」行携带。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
}

/// 首页 hero 主打项（对齐前端 `FeaturedItem`）。
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeaturedItemDto {
    /// 条目对外标识。
    pub id: String,
    /// 标题。
    pub title: String,
    /// 标题下的元信息片段，如 `["电影", "2025"]`。
    pub meta: Vec<String>,
    /// 规格标签，如 `["4K", "HDR10"]`；当前后端暂不提供，保留空数组占位。
    pub tags: Vec<String>,
    /// 简介；当前 browse 记录不含 overview，保留空串占位。
    pub overview: String,
    /// 背景剧照地址；为空时前端渲染占位块。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backdrop: Option<String>,
    /// 缩略图地址；为空时前端渲染占位块。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb: Option<String>,
    /// 详情路由类型，供 hero 的「播放/详情」跳转用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail_type: Option<String>,
}
