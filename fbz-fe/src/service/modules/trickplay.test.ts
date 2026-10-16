import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/service/request.ts", () => ({
  embyRequest: { get: vi.fn() },
  mediaImageUrl: (p?: string | null) => (p ? `${p}?api_key=t` : undefined),
}));

import { embyRequest } from "@/service/request.ts";
import { fetchTrickplay, trickplayFrame } from "@/service/modules/trickplay.ts";

const get = vi.mocked(embyRequest.get);
const info = {
  Width: 320,
  Height: 180,
  TileWidth: 10,
  TileHeight: 10,
  ThumbnailCount: 250,
  Interval: 10_000,
  Bandwidth: 1,
};

beforeEach(() => {
  get.mockReset();
});

describe("trickplay", () => {
  it("picks width closest to 320 and tolerates errors", async () => {
    get.mockResolvedValue({ data: [{ ...info, Width: 160 }, info] });
    expect((await fetchTrickplay("a"))?.Width).toBe(320);
    get.mockResolvedValue({ data: [] });
    expect(await fetchTrickplay("a")).toBeNull();
    get.mockImplementation(async () => {
      throw Object.assign(new Error("404"), { status: 404 });
    });
    await expect(fetchTrickplay("a")).resolves.toBeNull();
  });

  it("maps seconds to sheet and tile offset", () => {
    const f = trickplayFrame("a", info, 1234)!;
    // index 123 → sheet 1, tile 23 → col 3,row 2
    expect(f.url).toBe("/Videos/a/Trickplay/320/1.jpg?api_key=t");
    expect([f.offsetX, f.offsetY]).toEqual([-960, -360]);
    expect([f.sheetWidth, f.sheetHeight]).toEqual([3200, 1800]);
    expect(trickplayFrame("a", info, 1e9)!.url).toContain("/2.jpg");
    expect(trickplayFrame("a", info, -5)!.offsetX).toBe(-0);
  });
});
