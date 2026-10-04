import { afterEach, expect, it, vi } from "vitest";
import { withPlaybackDeadline } from "./webplayer.ts";
afterEach(() => vi.useRealTimers());
it("media preparation cannot remain pending indefinitely", async () => {
  vi.useFakeTimers();
  const waiting = withPlaybackDeadline(new Promise(() => {}), 15000);
  const assertion = expect(waiting).rejects.toThrow("PLAYBACK_TIMEOUT");
  await vi.advanceTimersByTimeAsync(15000);
  await assertion;
});
it("closing cancels preparation without waiting for the network timeout", async () => {
  const controller = new AbortController();
  const waiting = withPlaybackDeadline(new Promise(() => {}), 30000, controller.signal);
  controller.abort();
  await expect(waiting).rejects.toMatchObject({ name: "AbortError" });
});
it("successful preparation clears the deadline", async () => {
  vi.useFakeTimers();
  await expect(withPlaybackDeadline(Promise.resolve("ready"), 15000)).resolves.toBe("ready");
  expect(vi.getTimerCount()).toBe(0);
});
