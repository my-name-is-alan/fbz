// @vitest-environment jsdom
import { beforeEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { usePlaybackStore } from "./playback.ts";
import type { PlaybackItem } from "./playback.ts";
const mocks = vi.hoisted(() => ({ prepare: vi.fn() }));
vi.mock("@/service/modules/server.ts", () => ({ preparePlayback: mocks.prepare }));
beforeEach(() => {
  setActivePinia(createPinia());
  mocks.prepare.mockReset();
});
function fixture(): PlaybackItem {
  return {
    id: "first",
    type: "episode",
    title: "Series",
    playlist: ["first", "second", "third"].map((id, index) => ({
      id,
      title: "Series",
      seasonNumber: 1,
      episodeNumber: index + 1,
      duration: 120,
      serverItem: { Id: id, Name: id, Type: "Episode" },
    })),
  };
}
it("the last episode selection wins even if older network requests finish later", async () => {
  const store = usePlaybackStore();
  store.open(fixture());
  let second!: (item: PlaybackItem) => void;
  let third!: (item: PlaybackItem) => void;
  mocks.prepare
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          second = resolve;
        }),
    )
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          third = resolve;
        }),
    );
  const a = store.selectEpisode("second");
  const b = store.selectEpisode("third");
  third({ id: "third", type: "episode", title: "Third" });
  await b;
  second({ id: "second", type: "episode", title: "Second" });
  await a;
  expect(store.item?.id).toBe("third");
  expect(store.currentEpisodeIndex).toBe(2);
});
it("closing the player cancels a pending episode selection", async () => {
  const store = usePlaybackStore();
  store.open(fixture());
  let finish!: (item: PlaybackItem) => void;
  mocks.prepare.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const pending = store.selectEpisode("second");
  store.close();
  finish({ id: "second", type: "episode", title: "Second" });
  await pending;
  expect(store.isOpen).toBe(false);
});
it("loads a remote-route episode that has no legacy serverItem", async () => {
  const store = usePlaybackStore();
  const initial = fixture();
  initial.playlist = initial.playlist!.map(({ serverItem: _, ...episode }) => episode);
  store.open(initial);
  mocks.prepare.mockResolvedValue({
    id: "second",
    type: "episode",
    title: "Second",
    source: { uri: "/stream", proxyUri: "/bytes" },
    server: { playSessionId: "session", mediaSourceId: "source", startTicks: 100 },
  });
  await store.selectEpisode("second");
  expect(mocks.prepare).toHaveBeenCalledWith({ Id: "second", Name: "Series", Type: "Episode" });
  expect(store.item?.source?.proxyUri).toBe("/bytes");
  expect(store.item?.server?.playSessionId).toBe("session");
});
