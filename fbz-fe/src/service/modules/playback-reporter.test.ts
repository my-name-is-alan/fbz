// @vitest-environment jsdom
import { describe, it, expect, vi } from "vitest";
import { createPlaybackReporter } from "./playback-reporter.ts";

const base = {
  ItemId: "movie-1",
  MediaSourceId: "source-1",
  PlaySessionId: "play-1",
  PlayMethod: "DirectPlay",
  DeviceId: "browser-1",
};
describe("playback reporting", () => {
  it("orders start, progress, stop and ignores post-stop events", async () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const reporter = createPlaybackReporter(base, send);
    await reporter.report("progress", 0, true);
    expect(send).not.toHaveBeenCalled();
    await Promise.all([
      reporter.report("start", 12, false),
      reporter.report("progress", 18.5, true),
      reporter.report("stop", 20, true),
    ]);
    await reporter.report("progress", 0, true);
    expect(send.mock.calls.map((call) => call[0])).toEqual([
      "/emby/Sessions/Playing",
      "/emby/Sessions/Playing/Progress",
      "/emby/Sessions/Playing/Stopped",
    ]);
    expect(send.mock.calls[1]?.[1]).toMatchObject({
      PositionTicks: 185000000,
      IsPaused: true,
      PlaySessionId: "play-1",
    });
  });
  it("resuming a paused video updates the existing session", async () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const reporter = createPlaybackReporter(base, send);
    await reporter.report("start", 0, false);
    await reporter.report("start", 10, false);
    expect(send.mock.calls[1]?.[0]).toBe("/emby/Sessions/Playing/Progress");
  });
  it("a failed report is surfaced without blocking the final stop", async () => {
    const send = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValue(undefined);
    const reporter = createPlaybackReporter(base, send);
    await expect(reporter.report("start", 0, false)).rejects.toThrow("offline");
    await reporter.report("stop", 4, true);
    expect(send).toHaveBeenLastCalledWith(
      "/emby/Sessions/Playing/Stopped",
      expect.objectContaining({ PositionTicks: 40000000 }),
    );
  });
});
