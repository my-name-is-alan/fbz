# 媒体类型分类法归整设计（library_type / item_type / Emby CollectionType 对齐）

状态：草案（未实现）。本文修正一个**既有设计缺陷**：FBZ 现有三套互不一致的「媒体类型」分类法，无法表达用户的真实库结构（视频+图片混合库、家庭视频资料、TV Live 直播、剧集+影视混合库）。本文给出对齐后的统一分类法 + 纯增量（additive）迁移方案 + 全量影响面清单。落地前每张迁移、每个契约变更单独确认（遵 `CLAUDE.md` 迁移规约）。

本文是 `media-recognition-design.md` 的**上游依赖**：识别层的 `LibraryKind` 先验必须建立在一套真实可存储的库类型之上，否则 §1.5 的冲突裁决是悬空假设。

## 0. 问题陈述（带证据）

现状存在三套对不上的「类型」枚举：

| # | 分类法 | 取值 | 位置 |
| --- | --- | --- | --- |
| ① | `libraries.library_type`（库类型） | `movies / tv / music / mixed` | `migrations/0002_core_media.sql:102`、校验 `admin/routes.rs:2865` |
| ② | `media_items.item_type`（条目类型） | `folder / movie / series / season / episode / artist / album / track / collection` | `migrations/0002_core_media.sql:167` |
| ③ | 识别文档 `LibraryKind`（虚构第三套） | `Movie / Series / Anime / Documentary / Music / Mixed` | `media-recognition-design.md:129` |

四个具体缺陷：

1. **`LibraryKind::Anime` / `Documentary` 在 DB 里无法存储**——`library_type` 没有这两个值，识别核心却用它们作类型先验。Anime/Documentary 本质是**题材（genre）**而非**库类型**，混入 `LibraryKind` 是概念错位。
2. **`item_type` 缺图片/通用视频/直播类型**——没有 `photo`、`video`、`tvchannel`、`program`、`recording`。`item_type_for_file`（`scan/service.rs:1296-1301`）把非音频的一切强判为 `movie`：家庭录像、图片、综艺进库全变「电影」。
3. **`mixed` 是个语义糊掉的垃圾桶**——「视频+图片」「剧集+影视」是 Emby 里完全不同的两类（`homevideos` vs `mixed`），FBZ 压成了同一个值。
4. **Emby 边界透传 + 命名漂移**——`views.rs:109`、`media_folders.rs:305` 把 `library_type` 原样当 `CollectionType` 透传（零映射），但 DB 存 `tv` 而 Emby 期望 `tvshows`；同时 `dto/mod.rs:976` 又硬编码 `tvshows`。`tv` 与 `tvshows` 在同一代码库并存，已不自洽。

> Live TV 还有额外的结构问题：它在 Emby 是**独立子系统**（频道/EPG/录制），不是普通媒体库。`app.rs:1315` 的 `livetv` CollectionFolder 目前是 Emby 兼容层**硬造的空壳**，背后无任何数据模型。本文只把 `livetv` 纳入 `library_type` 取值并预留 `item_type`，**完整的频道/EPG/录制建模另立子系统**，不在本文范围。

## 1. 对齐后的统一分类法

设计原则：**`library_type` 直接对齐 Emby CollectionType 词汇表**，消除内部映射歧义；`item_type` 补齐物理条目种类；题材（Anime/Documentary）从类型轴剥离，不进任何枚举。

### 1.1 `library_type`（库类型，对齐 Emby CollectionType）

| 值 | 含义 | 对应 Emby CollectionType | 现状 |
| --- | --- | --- | --- |
| `movies` | 电影库 | `movies` | ✅ 已有 |
| `tvshows` | 剧集库 | `tvshows` | ⚠️ 现为 `tv`，需重命名（见 §2.3） |
| `music` | 音乐库 | `music` | ✅ 已有 |
| `homevideos` | 家庭视频 + 照片（视频+图片混合） | `homevideos` | ➕ 新增 |
| `mixed` | 电影 + 剧集混合库 | `mixed` | ✅ 已有（语义收窄为「影视混合」） |
| `livetv` | 直播电视（频道/EPG/录制入口） | `livetv` | ➕ 新增（数据模型另立子系统） |

> 你列的「自定义混合库（视频+图片）」→ `homevideos`；「家庭存储视频资料」→ 同样落 `homevideos`（家庭录像与照片同库）；「剧集影视混合库」→ `mixed`；「tv live」→ `livetv`。六个值完整覆盖需求，且每个都是 Emby 既有 CollectionType，客户端原生识别。

### 1.2 `item_type`（条目类型，补齐）

| 现有 | 新增 | 用途 |
| --- | --- | --- |
| `folder`/`movie`/`series`/`season`/`episode`/`artist`/`album`/`track`/`collection` | `photo` | 图片条目（homevideos 库） |
|  | `video` | 通用/家庭视频片段（无 movie 语义，不强行当电影） |
|  | `tvchannel` | 直播频道（livetv，预留） |
|  | `program` | EPG 节目（livetv，预留） |
|  | `recording` | 录制节目（livetv，预留） |

> `video` 是关键补充：它让 `item_type_for_file` 在 homevideos 库不再把家庭录像误判为 `movie`。`photo` 让图片首次能进库。直播三类先建枚举占位，物理模型随子系统落地。

### 1.3 题材轴（Anime/Documentary）何去何从

Anime/Documentary **不进 `library_type` 也不进 `item_type`**。它们是内容画像，应在识别层作为一个独立的提示信号（如 `RecognizedMedia.content_hint` 或库的可选 `genre_profile` 配置），用于：

- 识别启发式偏置（动漫库倾向 `[Group] Title - NN` 命名解析）；
- provider 查询时的 language/country 默认。

具体落点由 `media-recognition-design.md` 定义（见本文 §5 对该文档的修订要求）。**本文只主张：它们不是库类型，不占 `library_type` 枚举。**

## 2. 迁移方案（additive，逐张单独确认）

迁移编号衔接：工作区现有迁移到 `0079`。`media-recognition-design.md` §4.2 预占了 `0080_recognition_words.sql`。**本文占用 `0080`/`0081` 会与之冲突**——两文档需协调：建议本文用 `0080`/`0081`，识别词表顺延为 `0082`，并在 `media-recognition-design.md` 同步改号（见 §5）。

### 2.1 `0080_extend_library_type.sql`（放宽 library_type 约束）

放宽 CHECK 是纯增量：所有既有行仍合法。约束为匿名内联（`0002:102`），PG 自动命名 `libraries_library_type_check`。

```sql
-- 0080_extend_library_type.sql（additive：仅放宽 CHECK 取值域）
alter table libraries drop constraint if exists libraries_library_type_check;
alter table libraries add constraint libraries_library_type_check
    check (library_type in ('movies', 'tvshows', 'tv', 'music', 'homevideos', 'mixed', 'livetv'));
```

> 注意：此处**同时保留 `tv` 和 `tvshows`**，让 §2.3 的数据重命名可以分步、零停机进行（先放宽 → 再迁数据 → 最后可选收紧去掉 `tv`）。若决定不重命名 `tv`（见 §2.3 的取舍），则删掉 `tvshows` 那一项，库类型轴保持 `tv`，改为在 Emby 边界做映射。

### 2.2 `0081_extend_item_type.sql`（放宽 item_type 约束）

```sql
-- 0081_extend_item_type.sql（additive：仅放宽 CHECK 取值域）
alter table media_items drop constraint if exists media_items_item_type_check;
alter table media_items add constraint media_items_item_type_check
    check (item_type in (
        'folder', 'movie', 'series', 'season', 'episode',
        'artist', 'album', 'track', 'collection',
        'photo', 'video', 'tvchannel', 'program', 'recording'
    ));
```

纯增量，零既有行受影响，无需数据迁移。

### 2.3 `tv` → `tvshows` 重命名（唯一的非纯增量动作，需重点确认）

这是全案**唯一会改既有数据**的动作，单独决策。三种选项：

| 选项 | 做法 | 代价 | 推荐度 |
| --- | --- | --- | --- |
| A. 重命名存储值 | `update libraries set library_type='tvshows' where library_type='tv'` + 改校验/常量 | 改一处数据 + 多处字符串常量；与 Emby 词汇彻底统一 | ⭐ 倾向，根治命名漂移 |
| B. 存储保持 `tv`，边界映射 | 不动数据，在 Emby 边界加 `tv→tvshows` 映射函数 | 零数据迁移；但内部 `tv`/`tvshows` 双名长期并存 | 折中，低风险 |
| C. 维持现状 | 什么都不改 | 透传 bug 继续（客户端可能不认 `tv`） | ❌ 不可取 |

无论选 A 还是 B，§3 的 Emby 边界映射函数都应补上（A 之后映射变成恒等也值得保留，作防御）。

## 3. Emby 边界映射（修透传 bug，独立于迁移）

现状 `views.rs:109` 与 `media_folders.rs:305` 直接 `collection_type: record.library_type`，零映射。应抽出单一映射函数，集中处理 `library_type → CollectionType`：

```rust
// 建议落在 compat/emby（单一事实源），两个 route 都调它
fn library_type_to_collection_type(library_type: &str) -> &'static str {
    match library_type {
        "movies" => "movies",
        "tv" | "tvshows" => "tvshows",   // 兼容选项 B 的存储值
        "music" => "music",
        "homevideos" => "homevideos",
        "mixed" => "mixed",
        "livetv" => "livetv",
        _ => "mixed",                     // 未知库类型退化为 mixed（最宽容的 Emby 视图）
    }
}
```

同时 `dto/mod.rs:976` 硬编码的 `["movies","tvshows","music","mixed"]` content-type 列表应补 `homevideos`，并与本函数取值域保持一致（建议抽成共享常量，避免再次漂移）。

## 4. 全量影响面清单（落地阶段逐处核对）

| 层 | 位置 | 改动 |
| --- | --- | --- |
| schema | `migrations/0080`、`0081`（新增） | 放宽两个 CHECK |
| 校验 | `admin/routes.rs:2863 validate_library_type` | allowlist 加 `homevideos`/`mixed`/`livetv`（及 `tvshows`） |
| 扫描 | `scan/service.rs:1296 item_type_for_file` | 按库类型 + 扩展名产出 `photo`/`video`/`track`/`movie`；homevideos 库非音频→`video` 而非 `movie` |
| 扫描 | `scan/service.rs:1273 is_supported_media_file` | 若收图片，需补图片扩展名（jpg/png/...）；**确认是否纳入扫描** |
| Emby 边界 | `views.rs:109`、`media_folders.rs:305` | 改调 §3 映射函数 |
| Emby DTO | `dto/mod.rs:976` content_types 常量 | 加 `homevideos`，抽共享常量 |
| 测试 | `routes.rs:4046`、`scan/service.rs:1419`、`dto/mod.rs` 等 | 同步断言新取值 |
| 识别文档 | `media-recognition-design.md` | §5 修订 LibraryKind + 迁移号顺延 |

## 5. 对 `media-recognition-design.md` 的修订要求

1. **`LibraryKind` 对齐本文 `library_type`**：改为 `Movies / TvShows / Music / HomeVideos / Mixed / LiveTv`，删 `Anime`/`Documentary`。
2. **题材作独立信号**：动漫识别偏好改用 `content_hint`/`genre_profile`，不冒充库类型；§1.5 冲突裁决相应改写（不再依赖 `LibraryKind::Anime`）。
3. **迁移号顺延**：`recognition_words` 从 `0080` 改为 `0082`（本文占 `0080`/`0081`）。
4. **新增库类型的识别策略**：`homevideos` → 默认 `video`/`photo`，一般不联网查 provider；`livetv` → 不走文件名识别管线。

## 6. 已知风险与待确认项

- **`tv→tvshows` 重命名（§2.3）**：唯一改数据的动作，须明确选 A/B/C。选 A 要扫出所有硬编码 `"tv"` 字符串常量（`fanart.rs:51`、`tvdb.rs` 等是 **provider 内部路径**，与库类型无关，勿误改）。
- **图片是否纳入扫描**：`homevideos` 要存照片就得扩 `is_supported_media_file` 收图片扩展名，并确认 probe/缩略图链路能处理图片——可能牵出新工作量，单独评估。
- **livetv 子系统边界**：本文只占枚举位，频道/EPG/录制的表结构、抓取、播放是独立大项，不在本文。
- **迁移号与识别文档冲突**：`0080` 归属需两文档协调一致后再落盘，避免重号。
- **`mixed` 语义收窄的回归**：现存 `mixed` 库若实为「视频+图片」，重新归类到 `homevideos` 需人工确认，不可自动迁移（语义不可逆推）。
