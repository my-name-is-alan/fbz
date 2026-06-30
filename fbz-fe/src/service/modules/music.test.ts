import { beforeEach, describe, expect, it, vi } from "vitest";

// Mock 共享 axios 实例与图片地址拼接，纯断言调用形状/映射，不发网络。
vi.mock("@/service/request.ts", () => {
  return {
    request: { get: vi.fn() },
    // mediaImageUrl：无路径返回 undefined，有路径前缀加 token 占位（测试只验"被拼过"）。
    mediaImageUrl: (path?: string | null) => (path ? `IMG(${path})` : undefined),
  };
});

import { request } from "@/service/request.ts";
import {
  albumYearRange,
  fetchAlbumDetail,
  fetchArtistDetail,
  fetchArtists,
  formatDuration,
} from "@/service/modules/music.ts";

const mockedGet = vi.mocked(request.get);

beforeEach(() => {
  mockedGet.mockReset();
});

describe("formatDuration", () => {
  it("formats seconds as m:ss with zero-padding", () => {
    expect(formatDuration(210)).toBe("3:30");
    expect(formatDuration(5)).toBe("0:05");
    expect(formatDuration(60)).toBe("1:00");
  });

  it("returns empty string for missing or non-positive durations", () => {
    expect(formatDuration(undefined)).toBe("");
    expect(formatDuration(0)).toBe("");
    expect(formatDuration(-3)).toBe("");
  });
});

describe("albumYearRange", () => {
  it("collapses a single year and spans a range", () => {
    expect(albumYearRange([{ id: "a", title: "x", year: 2010 }])).toBe("2010");
    expect(
      albumYearRange([
        { id: "a", title: "x", year: 2010 },
        { id: "b", title: "y", year: 2020 },
      ]),
    ).toBe("2010 – 2020");
  });

  it("returns empty string when no album carries a year", () => {
    expect(albumYearRange([{ id: "a", title: "x" }])).toBe("");
    expect(albumYearRange([])).toBe("");
  });
});

describe("fetchArtists", () => {
  it("requests /music/artists with libraryId param", async () => {
    mockedGet.mockResolvedValue({ data: { items: [], total: 0 } });
    await fetchArtists("lib-1");
    expect(mockedGet).toHaveBeenCalledWith("/music/artists", {
      params: { libraryId: "lib-1" },
    });
  });
});

describe("fetchArtistDetail", () => {
  it("encodes id and resolves album posters to absolute urls", async () => {
    mockedGet.mockResolvedValue({
      data: {
        id: "art 1",
        name: "Queen",
        albums: [
          { id: "al1", title: "Opera", year: 1975, poster: "/Items/al1/Images/Primary" },
          { id: "al2", title: "Jazz", year: 1978 },
        ],
      },
    });
    const detail = await fetchArtistDetail("art 1");
    expect(mockedGet).toHaveBeenCalledWith("/music/artists/art%201");
    expect(detail.albums[0].poster).toBe("IMG(/Items/al1/Images/Primary)");
    // 无 poster 的专辑映射后为 undefined（交前端渲染占位块）。
    expect(detail.albums[1].poster).toBeUndefined();
  });
});

describe("fetchAlbumDetail", () => {
  it("resolves the album poster and preserves tracks", async () => {
    mockedGet.mockResolvedValue({
      data: {
        id: "al1",
        title: "Opera",
        year: 1975,
        poster: "/Items/al1/Images/Primary",
        tracks: [{ id: "t1", title: "Bohemian Rhapsody", duration: 354 }],
      },
    });
    const detail = await fetchAlbumDetail("al1");
    expect(mockedGet).toHaveBeenCalledWith("/music/albums/al1");
    expect(detail.poster).toBe("IMG(/Items/al1/Images/Primary)");
    expect(detail.tracks).toHaveLength(1);
    expect(detail.tracks[0].title).toBe("Bohemian Rhapsody");
  });
});
