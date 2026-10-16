// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/service/request.ts", () => ({
  embyRequest: { get: vi.fn() },
  mediaImageUrl: (p?: string | null) =>
    p ? `${p}${p.includes("?") ? "&" : "?"}api_key=t` : undefined,
}));

import { embyRequest } from "@/service/request.ts";
import {
  formatBadge,
  audioBookResume,
  type BookItem,
  fetchBooks,
  groupAudioBooks,
  pageUrl,
  resolveBookPath,
  rewriteChapterHtml,
} from "@/service/modules/books.ts";

const get = vi.mocked(embyRequest.get);

beforeEach(() => {
  get.mockReset();
});

describe("books service", () => {
  it("fetches Book/AudioBook items with folder names", async () => {
    get.mockResolvedValue({
      data: {
        Items: [
          { Id: "1", Name: "Dune", Type: "Book" },
          { Id: "2", Name: "02", Type: "AudioBook", ParentId: "f" },
          { Id: "f", Name: "Hobbit", Type: "Folder" },
        ],
      },
    });
    const out = await fetchBooks("u", "lib");
    expect(get).toHaveBeenCalledWith("/Users/u/Items", {
      params: expect.objectContaining({
        ParentId: "lib",
        IncludeItemTypes: "Book,AudioBook,Folder",
      }),
    });
    expect(out.map((b) => [b.id, b.type, b.folder])).toEqual([
      ["1", "Book", ""],
      ["2", "AudioBook", "Hobbit"],
    ]);
  });

  it("groups audiobooks by folder in natural order", () => {
    const g = groupAudioBooks([
      { id: "a", name: "10", type: "AudioBook", folder: "Hobbit" },
      { id: "b", name: "2", type: "AudioBook", folder: "Hobbit" },
      { id: "c", name: "Dune", type: "Book", folder: "x" },
    ]);
    expect(g).toEqual([
      {
        title: "Hobbit",
        tracks: [expect.objectContaining({ id: "b" }), expect.objectContaining({ id: "a" })],
      },
    ]);
  });

  it("resolves relative paths and builds page urls", () => {
    expect(resolveBookPath("OEBPS/Text/ch1.xhtml", "../Images/a%20b.png")).toBe(
      "OEBPS/Images/a b.png",
    );
    expect(pageUrl("x", 3)).toBe("/Books/x/Pages/3?api_key=t");
  });

  it("sanitizes chapter html and rewrites images", () => {
    const html = rewriteChapterHtml(
      "x",
      "OEBPS/ch1.xhtml",
      '<p onclick="evil()">Hi</p><script>alert(1)</script><img src="img/a.png"><a href="ch2.xhtml">next</a><iframe src="https://e"></iframe>',
    );
    expect(html).not.toContain("onclick");
    expect(html).not.toContain("<script");
    expect(html).not.toContain("<iframe");
    expect(html).toContain('src="/Books/x/Resource?Path=OEBPS%2Fimg%2Fa.png&amp;api_key=t"');
    expect(html).toContain("<a>next</a>");
  });
  it("picks the audiobook resume point from user data", () => {
    const t = (id: string, extra: Partial<BookItem> = {}): BookItem => ({
      id,
      name: id,
      type: "AudioBook",
      folder: "H",
      ...extra,
    });
    expect(audioBookResume([t("a"), t("b")])).toEqual({ index: 0, seconds: 0 });
    expect(
      audioBookResume([
        t("a", { played: true, lastPlayed: "2026-10-01T00:00:00Z" }),
        t("b", { positionSeconds: 95, lastPlayed: "2026-10-02T00:00:00Z" }),
      ]),
    ).toEqual({ index: 1, seconds: 95 });
    expect(
      audioBookResume([t("a", { played: true, lastPlayed: "2026-10-03T00:00:00Z" }), t("b")]),
    ).toEqual({ index: 1, seconds: 0 });
    // 无 LastPlayedDate：按顺序最后一个有进度的段落。
    expect(
      audioBookResume([t("a", { played: true }), t("b", { positionSeconds: 1.5 }), t("c")]),
    ).toEqual({
      index: 1,
      seconds: 1.5,
    });
    expect(audioBookResume([t("a", { played: true }), t("b")])).toEqual({ index: 1, seconds: 0 });
  });

  it("formatBadge shows source → target when converted", () => {
    expect(formatBadge({ Format: "epub", SourceFormat: "mobi", Converted: true })).toBe(
      "MOBI → EPUB",
    );
    expect(formatBadge({ Format: "cbz", SourceFormat: "cbr", Converted: true })).toBe("CBR → CBZ");
    expect(formatBadge({ Format: "epub", SourceFormat: "epub", Converted: false })).toBe("EPUB");
    expect(formatBadge({ Format: "pdf" })).toBe("PDF");
    expect(formatBadge(null)).toBe("");
  });
});
