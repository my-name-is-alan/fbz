import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/service/request.ts", () => ({
  embyRequest: {
    get: vi.fn(),
    post: vi.fn().mockResolvedValue({
      data: {
        GroupId: "g",
        GroupName: "n",
        State: "Idle",
        Participants: ["a"],
        LastUpdatedAt: "",
      },
    }),
  },
  getAccessToken: () => null,
}));

import { embyRequest } from "@/service/request.ts";
import { useSyncPlay, websocketUrl } from "@/composables/useSyncPlay.ts";

const post = vi.mocked(embyRequest.post);

beforeEach(() => {
  vi.useFakeTimers();
  post.mockClear();
});
afterEach(() => {
  vi.useRealTimers();
});

describe("useSyncPlay", () => {
  it("builds ws url from http origin", () => {
    expect(websocketUrl("https://h:1", "t k")).toBe("wss://h:1/embywebsocket?api_key=t%20k");
  });

  it("local controls pass through when not in a group", () => {
    const sp = useSyncPlay({ apply: vi.fn() });
    expect(sp.requestPlayPause(true)).toBe(false);
    expect(sp.requestSeek(3)).toBe(false);
    expect(post).not.toHaveBeenCalled();
  });

  it("create sets queue and routes controls to server; commands applied at When", async () => {
    const apply = vi.fn();
    const sp = useSyncPlay({ apply });
    await sp.create("n", "item1", 2);
    expect(post).toHaveBeenCalledWith("/SyncPlay/SetNewQueue", {
      PlayingQueue: ["item1"],
      PlayingItemPosition: 0,
      StartPositionTicks: 20_000_000,
    });
    expect(sp.active.value).toBe(true);
    expect(sp.requestPlayPause(true)).toBe(true);
    expect(post).toHaveBeenCalledWith("/SyncPlay/Unpause");
    expect(sp.requestSeek(1.5)).toBe(true);
    expect(post).toHaveBeenCalledWith("/SyncPlay/Seek", { PositionTicks: 15_000_000 });

    const when = new Date(Date.now() + 400).toISOString();
    sp.handleMessage({
      MessageType: "SyncPlayCommand",
      Data: { Command: "Pause", PositionTicks: 5, When: when },
    });
    expect(apply).not.toHaveBeenCalled();
    vi.advanceTimersByTime(400);
    expect(apply).toHaveBeenCalledWith(expect.objectContaining({ Command: "Pause" }));

    await sp.leave();
    expect(sp.active.value).toBe(false);
  });

  it("queues the season and switches items through the server", async () => {
    const sp = useSyncPlay({ apply: vi.fn() });
    await sp.create("n", "e2", 0, ["e1", "e2", "e3"]);
    expect(post).toHaveBeenCalledWith("/SyncPlay/SetNewQueue", {
      PlayingQueue: ["e1", "e2", "e3"],
      PlayingItemPosition: 1,
      StartPositionTicks: 0,
    });
    sp.handleMessage(
      JSON.stringify({
        MessageType: "SyncPlayGroupUpdate",
        Data: {
          GroupId: "g",
          Type: "PlayQueue",
          Data: {
            Playlist: [
              { ItemId: "e1", PlaylistItemId: "p1" },
              { ItemId: "e2", PlaylistItemId: "p2" },
            ],
            PlayingItemIndex: 1,
          },
        },
      }),
    );
    expect(sp.queueItemId.value).toBe("e2");
    expect(sp.requestItem("e1")).toBe(true);
    expect(post).toHaveBeenCalledWith("/SyncPlay/SetPlaylistItem", { PlaylistItemId: "p1" });
    expect(sp.requestItem("zz")).toBe(false);
    sp.reportBuffering();
    sp.reportBuffering();
    expect(post.mock.calls.filter((c) => c[0] === "/SyncPlay/Buffering")).toHaveLength(1);
    sp.reportReady();
    expect(post).toHaveBeenCalledWith("/SyncPlay/Ready", {});
  });

  it("shuffle / repeat / end-of-item go through the group", async () => {
    const sp = useSyncPlay({ apply: vi.fn() });
    expect(sp.toggleShuffle()).toBe(false);
    expect(sp.requestNext()).toBe(false);
    await sp.create("n", "e1", 0, ["e1", "e2"]);
    sp.handleMessage(
      JSON.stringify({
        MessageType: "SyncPlayGroupUpdate",
        Data: {
          GroupId: "g",
          Type: "PlayQueue",
          Data: {
            Playlist: [
              { ItemId: "e1", PlaylistItemId: "p1" },
              { ItemId: "e2", PlaylistItemId: "p2" },
            ],
            PlayingItemIndex: 0,
            ShuffleMode: "Sorted",
            RepeatMode: "RepeatAll",
          },
        },
      }),
    );
    expect(sp.state.value.repeatMode).toBe("RepeatAll");
    expect(sp.toggleShuffle()).toBe(true);
    await vi.runAllTimersAsync();
    expect(post).toHaveBeenCalledWith("/SyncPlay/SetShuffleMode", { Mode: "Shuffle" });
    expect(sp.cycleRepeat()).toBe(true);
    await vi.runAllTimersAsync();
    expect(post).toHaveBeenCalledWith("/SyncPlay/SetRepeatMode", { Mode: "RepeatOne" });
    expect(sp.requestNext()).toBe(true);
    expect(post).toHaveBeenCalledWith("/SyncPlay/NextItem", { PlaylistItemId: "p1" });
  });

  it("queue add/remove/move go through the group", async () => {
    const sp = useSyncPlay({ apply: vi.fn() });
    expect(sp.queueNext("e3")).toBe(false);
    expect(sp.removeQueueItem("p1")).toBe(false);
    await sp.create("n", "e1", 0, ["e1", "e2"]);
    expect(sp.queueNext("e3")).toBe(true);
    await vi.runAllTimersAsync();
    expect(post).toHaveBeenCalledWith("/SyncPlay/Queue", { ItemIds: ["e3"], Mode: "QueueNext" });
    expect(sp.moveQueueItem("p2", 0)).toBe(true);
    await vi.runAllTimersAsync();
    expect(post).toHaveBeenCalledWith("/SyncPlay/MovePlaylistItem", {
      PlaylistItemId: "p2",
      NewIndex: 0,
    });
    expect(sp.moveQueueItem("p2", -1)).toBe(false);
    expect(sp.removeQueueItem("p1")).toBe(true);
    await vi.runAllTimersAsync();
    expect(post).toHaveBeenCalledWith("/SyncPlay/RemoveFromPlaylist", {
      PlaylistItemIds: ["p1"],
      ClearPlaylist: false,
      ClearPlayingItem: false,
    });
  });
});
