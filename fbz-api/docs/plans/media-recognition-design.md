# 媒体识别子系统设计方案（文件名 → 结构化元数据 + 自定义识别词）

状态：阶段 0-5 全部已实现（`src/recognition/`：types/dict/rules/repository/routes + mod 管线，迁移 0082 recognition_words / 0083 容器去重 / 0084 media_files 画质列，扫描集成 + 层级归组 + QualityTags 落库 + probe 实测校正，MetadataLookup 契约扩展）。识别核心 + 规则引擎纯函数穷举单测全绿，迁移与扫描归组经 live-PG 验证。本文是 `backend-execution-goal.md` 第 3 节「媒体库扫描和元数据入库」与 `metadata-scraper-design.md` 之间缺失的**上游识别层**设计。`metadata-scraper-design.md` 解决「已知道查什么之后的 provider 编排」，本文解决「从乱七八糟的文件名/目录结构里认出这是什么剧、第几季第几集、什么版本」。两者串联：`scan` 发现文件 → **本层识别** → `metadata` registry 查询。

> **依赖前置**：本文的库类型先验（`LibraryKind`）建立在 `media-type-taxonomy-design.md` 定义的统一 `library_type` 之上。该文档把 `library_type` 对齐为 `movies / tvshows / music / homevideos / mixed / livetv`，并把题材（动漫/纪录片）从类型轴剥离为独立信号。本文 `LibraryKind` 与之对齐，**不再含 `Anime`/`Documentary`**。

## 0. 范围与定位

### 0.1 本层负责 / 不负责

**负责**：把一条物理路径（`ScanFile.path` + 其所在目录链）解析为 `RecognizedMedia`：媒体类型（movie/series/season/episode）、主标题、原始标题、年份、季号、集号（含区间）、分辨率、来源、编码、制作组/字幕组、版本标记（如 Director's Cut），并据此完成剧集层级归组（series→season→episode 的 `parent_id` 链）。

**不负责**：联网查询任何 provider（那是 `metadata` 层）；不负责 ffprobe 物理流探测（那是 `media/probe.rs`，识别层只用文件名/目录，probe 用于校正而非主识别）；不负责图片/语言策略（`metadata` 层）。

### 0.2 与 MoviePilot 的关系（许可证边界）

FBZ 为 **MIT**（`LICENSE`），MoviePilot 为 **GPL-3.0**。因此：

- ❌ **不得**复制、移植、或「翻译成 Rust」MoviePilot 任何源码——GPL 传染性会污染整个 MIT 仓库，演绎作品同样受限。
- ✅ **可借鉴**不受版权保护的部分：识别管线的分层思路、「自定义识别词」的交互形态与规则语义、正则模式所表达的命名约定知识。本文所有规则引擎为**净室重写**（clean-room）：依据公开行为语义独立实现，不参照其源码。

### 0.3 设计目标

1. 覆盖 §1 的多种命名情况，而非只认标准 Scene 命名。
2. 确定性、可单测、无副作用的纯解析核心（路径字符串 → `RecognizedMedia`），便于穷举 case 覆盖。
3. 管理员可配置的「自定义识别词」规则，对齐用户明确需求（自定义制作组/关键词/集偏移）。
4. 纯增量：不动 `media_items` schema，不破坏现有「文件名当标题」的回退路径（识别失败时退化为现状行为，零回归）。
5. 与 `metadata-scraper-design.md` 的 provider 契约对齐：识别产物可直接构造 `MetadataLookup`。

## 1. 必须适配的命名情况（穷举驱动设计）

识别层的复杂度全在「情况多」。下表是设计与测试的 case 清单，每类至少一个测试样本。解析器以「层级证据 + 多 token 提取 + 自定义识别词预处理」覆盖，而非单一大正则。

### 1.1 电影

| 情况 | 样本 | 期望产出 |
| --- | --- | --- |
| 标准 Scene | `Inception.2010.1080p.BluRay.x264-GROUP.mkv` | title=Inception, year=2010, res=1080p, source=BluRay, codec=x264, group=GROUP |
| 点分隔 + 多版本 | `Blade.Runner.2049.2017.2160p.UHD.BluRay.x265.HDR.mkv` | title=Blade Runner 2049, year=2017（注意标题内含 2049，需「最后一个合理年份」启发式） |
| 中文 + 英文双标题 | `盗梦空间.Inception.2010.1080p.mkv` | title=盗梦空间, original_title=Inception, year=2010 |
| 空格命名（非 Scene） | `The Matrix (1999).mp4` | title=The Matrix, year=1999 |
| 版本标记 | `Aliens.1986.Directors.Cut.1080p.mkv` | title=Aliens, year=1986, edition=Director's Cut |
| 无年份 | `Interstellar.mkv` | title=Interstellar, year=None（交 provider 消歧） |
| 目录名带元数据、文件名是裸名 | `Dune (2021)/movie.mkv` | 标题/年份来自**父目录** |
| 多碟/分卷 | `Movie.2020.CD1.avi` / `Movie.2020.part1.mkv` | 同一 media_item 的多 media_file，part 不进标题 |

### 1.2 剧集（TV / series）

| 情况 | 样本 | 期望产出 |
| --- | --- | --- |
| 标准 SxxExx | `Breaking.Bad.S01E05.1080p.mkv` | series=Breaking Bad, season=1, episode=5 |
| 多集合并 | `Show.S01E01-E03.mkv` | season=1, episode 区间 1..=3（多 episode item 或区间标记） |
| 1x05 风格 | `Show.1x05.mkv` | season=1, episode=5 |
| 仅集号（动漫常见） | `Show - 05.mkv`，目录 `Show/Season 1/` | season 来自目录，episode=5 |
| 季在目录、集在文件 | `Friends/Season 02/friends.e08.mkv` | season=2, episode=8 |
| 中文季集 | `庆余年.第01集.mkv` / `庆余年 第二季 第5集` | season/episode 从中文数字解析 |
| 合集季文件夹 | `Show/S01/...`、`Show/第一季/...` | 季号归一 |
| 绝对集号需偏移 | `Show - 38.mkv`（实为 S02E12，需 -26 偏移） | 由**自定义识别词**集偏移规则修正（§7） |
| 特别篇 / SP / OVA | `Show.S00E01.mkv`、`Show - SP01` | season=0（specials） |
| 日期型剧集（脱口秀） | `Daily.Show.2021.03.15.mkv` | 按播出日期归集（无 SxxExx，需日期模式） |

### 1.3 动漫（命名最混乱，单列）

| 情况 | 样本 | 期望产出 |
| --- | --- | --- |
| 字幕组前缀 + 方括号标签 | `[VCB-Studio] Fate [01][Ma10p_1080p][x265].mkv` | group=VCB-Studio, title=Fate, episode=1, res=1080p, codec=x265 |
| 多重方括号噪音 | `[Group][Title][12][BDRip][1080P][HEVC][FLAC].mkv` | 从一串 `[...]` 里区分「标题 token」与「技术 token」 |
| 版本号 v2 | `[Group] Title - 05v2 [1080p].mkv` | episode=5（v2 不计入集号） |
| 季用罗马数字 | `Title II - 03.mkv` | season=2 |

### 1.4 音乐

| 情况 | 样本 | 期望产出 |
| --- | --- | --- |
| 艺术家/专辑/曲目目录 | `Artist/Album (2020)/01 - Track.flac` | artist/album/track + track index |
| 文件名带轨号 | `01. Track Name.mp3` | track index=1, title=Track Name |

> 音乐沿用现有 `item_type='track'` 判定（`is_audio_file`），识别层只补轨号/标题清洗，层级归组按目录。音乐命名规范度高，规则集独立且简单。

### 1.5 退化与冲突

| 情况 | 策略 |
| --- | --- |
| 完全无法识别 | 退回现状：`title = file_stem`，`item_type` 按库类型默认，`metadata_status='pending'` 交 provider/人工 |
| 多种解释冲突（既像电影又像单集） | 由**库类型**（`libraries.library_type`）作先验：`movies` 库优先电影解释，`tvshows` 库优先剧集解释；`homevideos` 库优先 `video`（不强行套电影/剧集）。动漫题材偏好不靠库类型，由独立 `content_hint` 提示注入 |
| 目录证据与文件名证据矛盾 | 文件名优先于目录，目录作兜底（季号例外：目录季号更可信） |

## 2. 现状与差距（带证据）

- 扫描层零识别：`title_from_path`（`src/scan/service.rs:1316-1323`）只做 `file_stem().trim()`，文件名原样当标题，`Inception.2010.1080p.BluRay-X.mkv` 整串进 `media_items.title`。
- 类型判定只分两类：`item_type_for_file`（`src/scan/service.rs:1296-1302`）只返回 `track`（音频）或 `movie`，**永远不会产出 series/season/episode**，剧集层级形同虚设。
- 落库直写：`run_claimed_scan_job` 里 `insert into media_items(... item_type, title ...)`（`src/scan/service.rs:387-407`）直接绑定上面两个原始值，`parent_id`/`season_number`/`episode_number`/`production_year` 全部留空。
- **但 schema 早已就绪**：`media_items`（`migrations/0002_core_media.sql`）含 `parent_id`、`item_type in (folder/movie/series/season/episode/...)`、`season_number`、`episode_number`、`index_number`、`parent_index_number`、`original_title`、`production_year`，并有 `idx_media_items_parent_type_index_sort`（`migrations/0036`）支撑层级查询。识别层是**纯填充**，不需要 schema 改动（除自定义识别词规则表）。
- provider 契约不接剧集定位：`MetadataLookup`（`src/metadata/service.rs:741-750`）目前只有 `item_type/title/production_year/language/country/image_*`，**无 season/episode 字段**，识别出的集号无法下发给 provider 查剧集元数据。本文 §6.3 提出扩展。
- 无解析依赖：`Cargo.toml` 未引入 `regex`/`once_cell` 或任何文件名解析 crate。

## 3. 架构总览

新增模块 `src/recognition/`，定位为 `scan` 与 `metadata` 之间的**纯函数识别层 + 配置化规则**。三部分：

1. **识别核心（纯函数，无 IO）**：`recognize(input: RecognitionInput, rules: &RuleSet, library_type: LibraryKind) -> RecognizedMedia`。输入是路径链 + 库类型先验 + 已编译的识别词规则；输出结构化结果。完全确定性，便于穷举单测。内部分阶段：自定义识别词预处理 → token 化 → 类型判定 → 字段提取 → 目录/文件证据合并。
2. **规则配置层（DB + admin API）**：`recognition_words` 表存管理员自定义识别词；启动/变更时编译为内存 `RuleSet`（含预编译正则）。复用 `metadata-scraper-design.md` 同款「env/DB 合并 + 下次 job 重读」热更新模式。
3. **扫描集成层**：改 `scan/service.rs` 在 `insert into media_items` 之前调用识别核心，用 `RecognizedMedia` 填充 type/title/original_title/year/season/episode，并执行 §8 的层级归组（创建/复用 series、season 容器 item）。识别失败时退化为现状 `title_from_path` 行为。

```
ScanFile.path ─┐
父目录链        ├─► recognition::recognize(input, rules, lib_kind) ─► RecognizedMedia ─┐
library_type ──┘                                                                      │
                                                                                      ├─► 层级归组 + 写 media_items
RuleSet（编译自 recognition_words 表）────────────────────────────────────────────────┘
                                                                                      │
RecognizedMedia ─► 构造 MetadataLookup(+season/episode) ─► metadata registry 查询（既有）
```

**数据流不变量**：识别层不触库（除读规则）、不联网；所有外部证据（路径、库类型）经入参传入，保证核心可纯单测。规则编译失败（坏正则）不可阻断扫描——坏规则跳过并记 warn，识别照常进行。

## 4. 数据模型

### 4.1 识别核心类型（`src/recognition/types.rs`）

```rust
/// 识别核心的输入：一条文件路径 + 其所在的库根，拆为「文件名」与「祖先目录名链」。
pub struct RecognitionInput<'a> {
    pub file_stem: &'a str,        // 不含扩展名
    pub extension: Option<&'a str>,
    /// 从库根到文件的目录名链（不含库根本身、不含文件名），近→远顺序。
    /// 例：库根=/media/tv，文件=/media/tv/Friends/Season 02/x.mkv → ["Season 02", "Friends"]
    pub ancestors: &'a [&'a str],
}

pub enum LibraryKind { Movies, TvShows, Music, HomeVideos, Mixed, LiveTv }

pub enum RecognizedKind { Movie, Series, Season, Episode, Track, Photo, Video, Unknown }

/// 题材/内容画像提示——与库类型解耦的独立信号（动漫、纪录片等）。
/// 来源：库的可选配置或识别启发式；用于偏置解析与 provider 默认，**不参与类型判定**。
pub enum ContentHint { Anime, Documentary, None }

/// 识别结果。所有字段尽力而为；无法确定即 None，由上层退化或交 provider。
pub struct RecognizedMedia {
    pub kind: RecognizedKind,
    pub title: String,                    // 清洗后的主标题（查询用）
    pub original_title: Option<String>,   // 双标题时的另一语言标题
    pub year: Option<i32>,
    pub season: Option<i32>,              // specials = 0
    pub episodes: Vec<i32>,               // 多集合并时 >1；单集 len==1；电影为空
    pub edition: Option<String>,          // Director's Cut / Extended / Remastered
    pub release_group: Option<String>,    // 制作组 / 字幕组
    pub quality: QualityTags,             // 见下
    pub part: Option<i32>,                // CD1/part1，用于同 item 多 file
    pub content_hint: ContentHint,        // 动漫/纪录片题材提示（偏置解析，不定类型）
    pub confidence: Confidence,           // High / Medium / Low（决定是否退化）
    pub matched_rules: Vec<String>,       // 命中的自定义识别词 id，便于 admin 调试
}

/// 技术标签——不参与 provider 查询，但写入 media_files / 供 UI 展示。
pub struct QualityTags {
    pub resolution: Option<String>,       // 480p/720p/1080p/2160p
    pub source: Option<String>,           // BluRay/WEB-DL/HDTV/Remux
    pub video_codec: Option<String>,      // x264/x265/HEVC/AVC
    pub audio_codec: Option<String>,      // DTS/AC3/FLAC/AAC
    pub hdr: Option<String>,              // HDR/DV/HDR10+
}
```

> `QualityTags` 写到哪由实现阶段定：可落 `media_files` 现有列或新增轻量列（additive，单独确认）。识别层只产出，不决定存储。

### 4.2 自定义识别词规则表（迁移，需确认）

```sql
-- 0082_recognition_words.sql（additive，新增表）
-- 注：0080/0081 已被 media-type-taxonomy-design.md 占用（放宽 library_type / item_type 约束），故本表顺延至 0082。
create table if not exists recognition_words (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    kind text not null,              -- 'block' | 'replace' | 'offset' | 'replace_offset'
    pattern text not null,           -- 左件（屏蔽词 / 被替换词 / 前定位词）
    replacement text,                -- 右件（替换为；offset 类可空）
    anchor_after text,               -- offset 类：后定位词
    offset_expr text,                -- offset 类：集数偏移表达式，如 "-26" 或 "EP*2"
    is_regex boolean not null default false,
    enabled boolean not null default true,
    library_id bigint references libraries(id) on delete cascade,  -- null = 全局
    priority integer not null default 100,   -- 应用顺序，小者先
    note text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    check (kind in ('block', 'replace', 'offset', 'replace_offset')),
    check (length(trim(pattern)) > 0)
);
create index if not exists idx_recognition_words_scope
    on recognition_words (coalesce(library_id, 0), enabled, priority, id);
```

规则可全局或绑定单库（`library_id`）。编译时全局规则 + 该库规则按 `priority` 合并为一个 `RuleSet`。

## 5. 识别管线（分阶段，纯函数）

`recognize()` 内部按固定阶段流水，每阶段是独立可测的纯函数。顺序固定，后阶段依赖前阶段产出。

### 阶段 A — 自定义识别词预处理（§7 详述）

对 `file_stem`（及参与识别的目录名）先跑用户规则：屏蔽 → 替换。这一步在任何内置解析之前，因为它的作用就是把异常命名「纠正」成解析器认识的形态。集偏移规则记录下来，留到阶段 D 集号确定后应用。

### 阶段 B — token 化与技术标签剥离

把清洗后的字符串按分隔符（`.`、空格、`_`、`[]`、`()`）切成 token，再用内置词典剥离并归类**技术标签**（`QualityTags`）：分辨率、来源、编码、HDR、release group。

- 标签词典是**内置常量**（非用户规则），覆盖通用 PT/Scene 词汇。
- release group 启发式：Scene 命名取末尾 `-GROUP`；动漫取开头 `[Group]`。
- 剥离后剩余 token 即「标题候选 + 定位 token（年份/季集）」。

### 阶段 C — 类型判定

依「库类型先验 + 定位证据」决定 `RecognizedKind`：

- Music 库 / 音频扩展名 → `Track`（走音乐规则集，§1.4）。
- 命中 SxxExx / 1x05 / 中文季集 / `Show - NN` + 季目录 → `Episode`。
- Movie 库且无集号证据 → `Movie`。
- 库类型与证据冲突时按 §1.5 冲突表裁决（库类型先验 + 文件名优先）。

### 阶段 D — 字段提取

按类型抽取定位字段，多模式按优先级尝试（高特异性模式先）：

- **年份**：`(19|20)\d\d` 候选中取「最靠右且不在标题语义位置」的一个；标题内含数字（如 *Blade Runner 2049*）靠「年份 token 通常被分隔符独立包裹且接近技术标签」启发式区分。
- **季/集**：SxxExx → `1x05` → 中文「第N季/集」（中文数字转阿拉伯）→ 目录季号 + 文件裸集号 → 多集区间 `E01-E03`。`v2` 版本号、`part`/`CD` 分卷不计入集号。
- **edition / part**：版本词典与分卷模式。
- 提取后应用阶段 A 暂存的**集偏移**规则（绝对集号 → 季内集号修正）。

### 阶段 E — 目录 / 文件证据合并

文件名缺失的字段用祖先目录补：

- series 标题：文件无标题（如 `- 05.mkv`）时取最近含标题的祖先目录。
- 季号：`Season 02` / `S01` / `第一季` 目录覆盖文件缺省（季号目录证据优先于文件，见 §1.5）。
- 电影年份：目录 `Dune (2021)` 补文件裸名。

### 阶段 F — 置信度与退化

依命中证据数评 `Confidence`。`Low`（仅剥离出标题、无任何定位证据且库类型无法定型）→ 上层退化为 `title_from_path` 现状行为，`metadata_status='pending'`。保证识别层永不比现状更差。

## 6. crate 选型与契约扩展

### 6.1 解析 crate（新增依赖，MIT/Apache）

不从零写正则，复用成熟解析库，但都需 `Cargo.toml` 显式引入并锁版本（§项目依赖规范）：

- `regex`（MIT/Apache-2.0）：阶段 A 自定义识别词、阶段 D 各定位模式的底座。**必需**。
- `once_cell`（MIT/Apache-2.0）：内置技术标签词典/模式的进程内惰性编译。**必需**（或用 `std::sync::LazyLock`，edition 2024 可用，倾向后者免依赖）。
- `torrent-name-parser`（MIT）或等价：候选用于阶段 B/D 的 Scene 命名提取，**待实现期评估**——若其覆盖度好可减少自写正则；若引入过多间接依赖则只自写。**先标记为可选，不锁死**。
- 动漫命名（`[Group] Title - NN`）：优先自写小型解析（方括号 token 分类），避免引入维护不活跃的 anitomy 绑定。**默认不引入**额外 crate。

> 选型原则：先用 `regex` + `LazyLock` 自写核心（完全可控、可单测），仅当某类命名自写成本过高再评估专用 crate。本文不在草案阶段锁定 `torrent-name-parser`。

### 6.2 内置词典作为常量

技术标签、版本、季集模式等**内置知识**为代码内常量（`LazyLock<Vec<(Regex, Tag)>>`），与用户的 `recognition_words` 规则分离：内置词典管「通用命名约定」，用户规则管「本地化异常修正」。

### 6.3 provider 查询契约扩展（关键）

`MetadataLookup`（`src/metadata/service.rs:741-750`）需新增剧集定位字段，否则识别出的 season/episode 无法下发：

```rust
// MetadataLookup 增补
pub season: Option<i32>,
pub episode: Option<i32>,
pub original_title: Option<String>,
```

- 这是 `metadata` 层的 additive 改动，需与 `metadata-scraper-design.md` 协同：provider 的 `match_item` 在 `item_type == "episode"` 时用 (series_title, season, episode) 查询剧集元数据。
- `build_lookup`（`src/metadata/service.rs:712`）从对应 `media_items` 行（识别已写入 season/episode）读取填充。
- **本文只声明契约缺口与字段形状**；provider 端如何消费属 `metadata-scraper-design.md` 范畴，落地时两文档同步更新。

## 7. 自定义识别词引擎（净室重写，对齐 MoviePilot 形态）

用户明确要的「自定义制作组/关键词/集偏移」。借鉴 MoviePilot 的交互形态与规则语义（公开行为，非源码），独立实现四类规则。规则以行文本形式由管理员录入，解析为 `recognition_words` 行；引擎在 §5 阶段 A/D 应用。

### 7.1 四类规则与语法

| 类型 | `kind` | 录入语法 | 含义 | 应用阶段 |
| --- | --- | --- | --- | --- |
| 屏蔽词 | `block` | `<被屏蔽内容>` | 从待识别字符串删除该片段（噪音、广告标签） | A |
| 替换词 | `replace` | `<被替换> => <替换为>` | 把左件替换为右件（纠正错误命名、统一别名） | A |
| 集数偏移 | `offset` | `<前定位> <> <后定位> >> <偏移表达式>` | 在前/后定位词之间的集号上施加偏移 | D |
| 替换 + 偏移 | `replace_offset` | `<被替换> => <替换为> && <前定位> <> <后定位> >> <偏移表达式>` | 先替换再按定位偏移，`&&` 分隔两段 | A + D |

- 分隔符常量：`=>`（替换）、`<>`（前后定位之间）、`>>`（定位与偏移表达式之间）、`&&`（替换段与偏移段之间）。这些是**语义约定**，非抄代码。
- `is_regex=true` 时左件/定位词按正则解释；否则按字面（自动转义）。
- 录入解析在 admin API 侧完成（拆出 `kind`/`pattern`/`replacement`/`anchor_after`/`offset_expr`），DB 存结构化列而非原始行，便于校验与编辑。

### 7.2 集数偏移表达式

`offset_expr` 支持：

- 常量偏移：`-26`、`+12`（绝对集号 ↔ 季内集号最常见）。
- 线性表达式：以 `EP` 代表识别出的原集号，如 `EP-26`、`EP*2-1`（双季合并/分割）。求值用极简表达式解析器（仅 `+ - *` 与整数、`EP`），**不引入通用表达式 crate**，自写约 30 行带边界保护。
- 偏移仅在「前定位 ~ 后定位」窗口命中时生效；定位词可为空表示无窗口约束（整串生效）。

### 7.3 应用语义与安全

- 顺序：同 `priority` 内 block → replace → offset；跨规则按 `priority` 升序。命中记入 `RecognizedMedia.matched_rules`，admin「测试识别」界面回显，便于调规则。
- 幂等与防爆：replace 不递归重扫（避免 A→B→A 循环）；单条规则对单个字符串最多作用一次匹配位置集合。
- 正则安全：编译期校验 + 大小限制；用 `regex` crate（线性时间，无灾难性回溯），坏正则在编译 `RuleSet` 时跳过并 warn，不阻断扫描（§3 不变量）。
- 偏移越界（算出负集号/0）：丢弃该偏移结果，保留原集号并记 warn，绝不写非法集号。

### 7.4 Admin API（`/api/admin/recognition/*`，server-admin 门控）

复用既有 admin 权限模式（每 handler 走 `authenticate_admin`/`can_manage_server`，被 `every_admin_route_handler_enforces_server_admin` 守卫覆盖）：

- `GET /api/admin/recognition/words` — 列出规则（全局 + 按库过滤）。
- `POST /api/admin/recognition/words` — 新增/校验一条规则（服务端解析录入语法，回报语法错误）。
- `PUT/DELETE /api/admin/recognition/words/{id}` — 编辑/删除。
- `POST /api/admin/recognition/test` — **核心调试入口**：传一个样例文件名（可选库类型），返回 `RecognizedMedia` 全字段 + `matched_rules`，让管理员不扫库即可验证规则效果。

## 8. 剧集层级归组

识别出 `Episode` 后，扫描集成层要把扁平文件组织成 series→season→episode 树（填 `parent_id`）。

- **容器 item 复用/创建**：对 (library_id, series_title) 查找或创建 `item_type='series'` 容器（`is_virtual=true` 直到 provider 富化）；对 (series_id, season) 创建 `item_type='season'` 容器；episode 的 `parent_id` 指向 season（或 series，若无季）。
- **去重键**：series 容器按 (library_id, 规范化 series_title) 去重，避免同剧多容器；并发扫描用 `insert ... on conflict` 或事务内 select-for-update（与现有 scan 事务模式一致）。
- **specials**：season=0 容器。
- **多集合并**（`episodes.len() > 1`）：首版建一个代表 episode item（index_number 取首集）+ 多个 media_file，或按区间建多 episode item——**落地前确认**取哪种（影响 Emby 客户端展示）。
- **电影**：无层级，`item_type='movie'`，`parent_id=null`，多碟为多 media_file 挂同一 movie item（`part` 区分）。
- 现状「一文件一 movie item」的回退路径在识别 `Unknown`/`Low` 时保留，零回归。

> 归组改动集中在 `scan/service.rs` 的 `insert into media_items` 段（`src/scan/service.rs:387-407`），是本设计对现有代码的主要侵入点，需仿现有事务/幂等模式重写。

## 9. 测试策略

- **识别核心穷举单测**：§1 每张表的每个样本一个用例，断言 `RecognizedMedia` 全字段。纯函数无 IO，可大批量 table-driven 测试。这是本子系统的质量主轴。
- **退化保证**：构造无法识别的样本，断言退回 `title_from_path` 等价结果，证明零回归。
- **识别词引擎**：四类规则各覆盖；集偏移表达式（常量/线性/越界丢弃）；规则顺序与 `priority`；坏正则跳过不 panic；replace 不递归。
- **目录证据合并**：季在目录/集在文件、电影年份在目录等跨层级用例。
- **库类型先验**：同一文件名在 movie 库 vs series 库得到不同 `kind`。
- **归组**：模拟同剧多集多文件，断言 series/season 容器复用与 `parent_id` 链；并发去重（事务测试）。
- **迁移**：`recognition_words` 表结构测试 + 本地 dockerized PG 实跑（仿 `metadata-scraper-design.md` live-PG 校验模式）。
- **admin API**：路由存在 + server-admin 门控（`every_admin_route_handler_enforces_server_admin` 自动覆盖）；`/recognition/test` 端到端返回结构断言。
- **契约对齐**：`MetadataLookup` 新增字段被 `build_lookup` 正确填充（与 `metadata` 层联测）。

每阶段以 `cargo fmt --check` / `cargo test --lib` / `cargo build --lib` 全绿为完成判据。

## 10. 分步实施计划（每步独立可验证、可单独确认）

- **阶段 0 — 识别核心骨架（无集成、无 DB）**：建 `src/recognition/` + `types.rs` + `recognize()` 纯函数 + 内置词典常量；实现 §1.1 电影 + §1.2 标准 SxxExx 两类，穷举单测。引入 `regex` 依赖。**不接扫描，零行为变更**。
- **阶段 1 — 扩展识别覆盖**：补动漫（§1.3）、中文季集、目录证据合并（§5 阶段 E）、多集/specials/分卷。全部靠单测驱动，仍不接扫描。
- **阶段 2 — 自定义识别词引擎 + DB**：迁移 0082 `recognition_words`；录入语法解析器；四类规则 + 偏移表达式求值；`RuleSet` 编译（坏规则跳过）；admin API（含 `/recognition/test`）。**先确认迁移**。
- **阶段 3 — 扫描集成 + 层级归组**：改 `scan/service.rs` 调识别核心，重写 `insert into media_items` 段做 series/season/episode 归组（§8），保留退化路径。这步是主要侵入点，需重点回归测试现有扫描行为。
- **阶段 4 — provider 契约对齐**：扩 `MetadataLookup`（season/episode/original_title），与 `metadata-scraper-design.md` 协同让 provider 查询剧集元数据。**两文档同步更新**。
- **阶段 5（可选）— probe 校正与 QualityTags 落库**：用 `media/probe.rs` 的实测分辨率/编码校正文件名标签的冲突；`QualityTags` 持久化（additive 列，单独确认）。

## 11. 已知风险与待确认项

- **schema 改动**：核心识别零 schema 改动；新增 `recognition_words`（0082）为 additive 单独确认；`QualityTags` 落库（阶段 5）若加列需再确认。
- **归组的并发与幂等**：series/season 容器去重在并发扫描下易产生重复容器，必须用事务 + on-conflict，是阶段 3 最高风险点，需仿现有 scan 事务模式并加并发测试。
- **多集合并的展示语义**：`E01-E03` 建一个还是多个 episode item 影响 Emby 客户端，落地前确认。
- **识别误判 vs 退化**：宁可退化（交 provider/人工）也不可写错误的 season/episode 污染层级。置信度阈值需在真实媒体库样本上校准。
- **provider 契约耦合**：阶段 4 改 `MetadataLookup` 跨子系统，须与 `metadata-scraper-design.md` 同步，避免两文档漂移。
- **crate 选型未锁定**：`torrent-name-parser` 等是否引入留待阶段 0/1 实测覆盖度后定；草案默认 `regex` + 自写。
- **中文数字/罗马数字解析**：「第二季」「Title II」需小型数字解析器，边界（廿/两/零）需测试覆盖。

