import { beforeEach, describe, expect, it, vi } from "vitest";

// Mock emby 根面客户端：断言上报端点路径 + body 形状，不发网络。
vi.mock("@/service/request.ts", () => ({
  embyRequest: { post: vi.fn().mockResolvedValue({ data: {} }) },
}));

import { embyRequest } from "@/service/request.ts";
import {
  newPlaySessionId,
  reportPlaybackProgress,
  reportPlaybackStart,
  reportPlaybackStopped,
} from "@/service/modules/playbackReport.ts";

const mockedPost = vi.mocked(embyRequest.post);

beforeEach(() => {
  mockedPost.mockClear();
});

describe("reportPlaybackStart", () => {
  it("posts to /Sessions/Playing with position in ticks", async () => {
    await reportPlaybackStart({ itemId: "t1", playSessionId: "s1", positionSeconds: 0 });
    expect(mockedPost).toHaveBeenCalledWith("/Sessions/Playing", {
      ItemId: "t1",
      PlaySessionId: "s1",
      PlayMethod: "DirectStream",
      PositionTicks: 0,
      IsPaused: false,
    });
  });
});

describe("reportPlaybackProgress", () => {
  it("converts seconds to ticks (×10_000_000) and carries pause state", async () => {
    await reportPlaybackProgress({
      itemId: "t1",
      playSessionId: "s1",
      positionSeconds: 30,
      isPaused: true,
    });
    expect(mockedPost).toHaveBeenCalledWith("/Sessions/Playing/Progress", {
      ItemId: "t1",
      PlaySessionId: "s1",
      PlayMethod: "DirectStream",
      EventName: "TimeUpdate",
      PositionTicks: 30 * 10_000_000,
      IsPaused: true,
    });
  });
});

describe("reportPlaybackStopped", () => {
  it("posts the final position to /Sessions/Playing/Stopped", async () => {
    await reportPlaybackStopped({ itemId: "t1", playSessionId: "s1", positionSeconds: 125 });
    expect(mockedPost).toHaveBeenCalledWith("/Sessions/Playing/Stopped", {
      ItemId: "t1",
      PlaySessionId: "s1",
      PlayMethod: "DirectStream",
      PositionTicks: 125 * 10_000_000,
    });
  });

  it("rounds and clamps negative positions to zero ticks", async () => {
    await reportPlaybackStopped({ itemId: "t1", playSessionId: "s1", positionSeconds: -5 });
    expect(mockedPost).toHaveBeenCalledWith(
      "/Sessions/Playing/Stopped",
      expect.objectContaining({ PositionTicks: 0 }),
    );
  });
});

describe("newPlaySessionId", () => {
  it("produces unique-ish session ids with the web prefix", () => {
    const a = newPlaySessionId();
    const b = newPlaySessionId();
    expect(a).toMatch(/^fbz-web-/);
    expect(a).not.toBe(b);
  });
});
