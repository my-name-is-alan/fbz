/**
 * 书籍 / 有声书 C 端 service（Jellyfin `books` 库）。
 * - 列表：`GET /Users/{uid}/Items?ParentId=&IncludeItemTypes=Book,AudioBook&Recursive=true`
 * - 阅读：fbz 扩展 `GET /Books/{id}/Manifest|Chapters/{n}|Pages/{n}|Resource?Path=|File`
 * 有声书走既有音频直出（`/Audio/{id}/universal`）+ 音乐播放条。
 */
import { embyRequest, mediaImageUrl } from "@/service/request.ts";

export interface BookItem {
  id: string;
  name: string;
  type: "Book" | "AudioBook";
  /** 所在书目录名（有声书按服务端 folder 容器归组为一本书）。 */
  folder: string;
  poster?: string;
  /** 作者 / 年份（电子书元数据，扫描期从 OPF/ComicInfo 提取）。 */
  authors?: string[];
  year?: number;
  /** 服务端记录的播放位置（秒，有声书续听用）。 */
  positionSeconds?: number;
  played?: boolean;
  lastPlayed?: string;
}

interface ItemDto {
  Id: string;
  Name: string;
  Type: string;
  ParentId?: string | null;
  ImageTags?: Record<string, string> | null;
  ProductionYear?: number | null;
  People?: { Name: string; Type?: string }[] | null;
  UserData?: {
    PlaybackPositionTicks?: number;
    Played?: boolean;
    LastPlayedDate?: string | null;
  } | null;
}

export interface BookManifest {
  Id: string;
  Format: string;
  Readable: boolean;
  Title?: string | null;
  Chapters?: { Index: number; Path: string; Title?: string | null }[];
  PageCount?: number;
  /** 原始文件格式（mobi/azw3/cbr 等服务端转换前的格式）。 */
  SourceFormat?: string | null;
  /** 是否经服务端转换（mobi/azw3 → epub、cbr → cbz）。 */
  Converted?: boolean;
}

/** 阅读器格式标识：转换过显示「MOBI → EPUB」，否则显示原格式（大写）。 */
export function formatBadge(
  m: Pick<BookManifest, "Format" | "SourceFormat" | "Converted"> | null | undefined,
): string {
  if (!m?.Format) return "";
  const target = m.Format.toUpperCase();
  const source = (m.SourceFormat || m.Format).toUpperCase();
  return m.Converted && source !== target ? `${source} → ${target}` : source;
}

export interface BookChapter {
  Index: number;
  Path: string;
  ChapterCount: number;
  Html: string;
}

export async function fetchBooks(userId: string, libraryId: string): Promise<BookItem[]> {
  const { data } = await embyRequest.get<{ Items?: ItemDto[] }>(
    `/Users/${encodeURIComponent(userId)}/Items`,
    {
      params: {
        ParentId: libraryId,
        Recursive: true,
        IncludeItemTypes: "Book,AudioBook,Folder",
        SortBy: "SortName",
        Fields: "People,ProductionYear",
        EnableUserData: true,
        // 列表默认不带图片 tag：显式要 Primary，封面（扫描期提取）才会出现。
        EnableImages: true,
        EnableImageTypes: "Primary",
      },
    },
  );
  const items = data.Items ?? [];
  const folders = new Map(items.filter((it) => it.Type === "Folder").map((it) => [it.Id, it.Name]));
  return items
    .filter((it) => it.Type === "Book" || it.Type === "AudioBook")
    .map((it) => ({
      id: it.Id,
      name: it.Name,
      type: it.Type as BookItem["type"],
      folder: (it.ParentId && folders.get(it.ParentId)) || "",
      poster: it.ImageTags?.Primary
        ? mediaImageUrl(`/Items/${encodeURIComponent(it.Id)}/Images/Primary`)
        : undefined,
      authors: (it.People ?? [])
        .filter((p) => p.Type === "Writer" || p.Type === "Author")
        .map((p) => p.Name),
      year: it.ProductionYear ?? undefined,
      positionSeconds: (it.UserData?.PlaybackPositionTicks ?? 0) / 10_000_000,
      played: it.UserData?.Played ?? false,
      lastPlayed: it.UserData?.LastPlayedDate ?? undefined,
    }));
}

/**
 * 有声书续听点：最近播放过且未播完的那段 → 从记录位置继续；
 * 最近播放的那段已播完 → 下一段从头；从未播放 → 第一段从头。
 */
export function audioBookResume(tracks: BookItem[]): { index: number; seconds: number } {
  let latest = -1;
  for (let i = 0; i < tracks.length; i += 1) {
    const t = tracks[i]!;
    if (!t.lastPlayed) continue;
    if (latest < 0 || t.lastPlayed > tracks[latest]!.lastPlayed!) latest = i;
  }
  // 列表接口可能不带 LastPlayedDate：退回“顺序上最后一个有进度/已播完的段落”。
  if (latest < 0) {
    for (let i = tracks.length - 1; i >= 0; i -= 1) {
      const t = tracks[i]!;
      if (t.played || (t.positionSeconds ?? 0) > 0) {
        latest = i;
        break;
      }
    }
  }
  if (latest < 0) return { index: 0, seconds: 0 };
  const track = tracks[latest]!;
  if (!track.played && (track.positionSeconds ?? 0) > 0) {
    return { index: latest, seconds: track.positionSeconds ?? 0 };
  }
  if (track.played && latest + 1 < tracks.length) return { index: latest + 1, seconds: 0 };
  return { index: latest, seconds: 0 };
}

/** 有声书按目录归组（同一目录多段音频 = 一本书），组内按名称自然序。 */
export function groupAudioBooks(items: BookItem[]): { title: string; tracks: BookItem[] }[] {
  const groups = new Map<string, BookItem[]>();
  for (const it of items) {
    if (it.type !== "AudioBook") continue;
    const key = it.folder || it.name;
    groups.set(key, [...(groups.get(key) ?? []), it]);
  }
  const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
  return [...groups.entries()]
    .map(([title, tracks]) => ({
      title,
      tracks: tracks.sort((a, b) => collator.compare(a.name, b.name)),
    }))
    .sort((a, b) => collator.compare(a.title, b.title));
}

export async function fetchManifest(itemId: string): Promise<BookManifest> {
  const { data } = await embyRequest.get<BookManifest>(
    `/Books/${encodeURIComponent(itemId)}/Manifest`,
  );
  return data;
}

export async function fetchChapter(itemId: string, index: number): Promise<BookChapter> {
  const { data } = await embyRequest.get<BookChapter>(
    `/Books/${encodeURIComponent(itemId)}/Chapters/${index}`,
  );
  return data;
}

export function pageUrl(itemId: string, index: number): string | undefined {
  return mediaImageUrl(`/Books/${encodeURIComponent(itemId)}/Pages/${index}`);
}

export function fileUrl(itemId: string): string | undefined {
  return mediaImageUrl(`/Books/${encodeURIComponent(itemId)}/File`);
}

/** 把章节内相对 href 解析为 epub 内完整路径。 */
export function resolveBookPath(chapterPath: string, href: string): string {
  const base = chapterPath.split("/").slice(0, -1);
  for (const seg of href.split("#")[0]!.split("/")) {
    if (!seg || seg === ".") continue;
    if (seg === "..") base.pop();
    else base.push(decodeURIComponent(seg));
  }
  return base.join("/");
}

/**
 * 章节 HTML → 可安全嵌入的 HTML：图片改走 Resource 端点；移除事件属性 / 外链脚本 / iframe；
 * 链接只保留文本（章节跳转由阅读器按钮负责）。
 */
export function rewriteChapterHtml(itemId: string, chapterPath: string, html: string): string {
  const doc = new DOMParser().parseFromString(`<div id="root">${html}</div>`, "text/html");
  const root = doc.getElementById("root")!;
  root.querySelectorAll("script,iframe,object,embed,link,style,form").forEach((el) => el.remove());
  root.querySelectorAll("*").forEach((el) => {
    // 复制一份：删除属性会改动 live NamedNodeMap。
    for (const attr of Array.from(el.attributes)) {
      if (/^on/i.test(attr.name) || /^\s*javascript:/i.test(attr.value))
        el.removeAttribute(attr.name);
    }
  });
  const rewrite = (el: Element, attr: string) => {
    const v = el.getAttribute(attr);
    if (!v || /^(data:|https?:)/i.test(v)) return;
    const path = resolveBookPath(chapterPath, v);
    el.setAttribute(
      attr,
      mediaImageUrl(
        `/Books/${encodeURIComponent(itemId)}/Resource?Path=${encodeURIComponent(path)}`,
      ) ?? "",
    );
  };
  root.querySelectorAll("img[src]").forEach((el) => rewrite(el, "src"));
  root.querySelectorAll("image").forEach((el) => {
    if (el.getAttribute("xlink:href")) rewrite(el, "xlink:href");
    if (el.getAttribute("href")) rewrite(el, "href");
  });
  root.querySelectorAll("a[href]").forEach((el) => el.removeAttribute("href"));
  return root.innerHTML;
}
