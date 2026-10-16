import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/service/request.ts", () => ({
  embyRequest: { get: vi.fn(), post: vi.fn() },
}));

import { embyRequest } from "@/service/request.ts";
import {
  movePlaylistItem,
  queueItems,
  removeFromPlaylist,
  applySyncPlayMessage,
  createGroup,
  emptySyncPlayState,
  joinGroup,
  listGroups,
  msUntil,
  nextRepeatMode,
  seek,
  setNewQueue,
} from "@/service/modules/syncplay.ts";

const get = vi.mocked(embyRequest.get);
const post = vi.mocked(embyRequest.post);

beforeEach(() => {
  get.mockReset();
  post.mockReset();
});

const update = (Type: string, Data: unknown) =>
  JSON.stringify({ MessageType: "SyncPlayGroupUpdate", Data: { GroupId: "g", Type, Data } });

describe("syncplay REST", () => {
  it("lists, creates, joins and queues with JF body shapes", async () => {
    get.mockResolvedValue({ data: [{ GroupId: "g" }] });
    expect(await listGroups()).toHaveLength(1);
    post.mockResolvedValue({ data: { GroupId: "g", GroupName: "x" } });
    expect((await createGroup("x")).GroupId).toBe("g");
    expect(post).toHaveBeenCalledWith("/SyncPlay/New", { GroupName: "x" });
    await joinGroup("g");
    expect(post).toHaveBeenCalledWith("/SyncPlay/Join", { GroupId: "g" });
    await setNewQueue(["a", "b"], 1, 50);
    expect(post).toHaveBeenCalledWith("/SyncPlay/SetNewQueue", {
      PlayingQueue: ["a", "b"],
      PlayingItemPosition: 1,
      StartPositionTicks: 50,
    });
    await seek(-5);
    expect(post).toHaveBeenCalledWith("/SyncPlay/Seek", { PositionTicks: 0 });
  });

  it("queue add/remove/move use JF body shapes", async () => {
    await queueItems(["a", "b"], "QueueNext");
    expect(post).toHaveBeenCalledWith("/SyncPlay/Queue", {
      ItemIds: ["a", "b"],
      Mode: "QueueNext",
    });
    await removeFromPlaylist(["p1"]);
    expect(post).toHaveBeenCalledWith("/SyncPlay/RemoveFromPlaylist", {
      PlaylistItemIds: ["p1"],
      ClearPlaylist: false,
      ClearPlayingItem: false,
    });
    await movePlaylistItem("p2", 0);
    expect(post).toHaveBeenCalledWith("/SyncPlay/MovePlaylistItem", {
      PlaylistItemId: "p2",
      NewIndex: 0,
    });
  });

  it("listGroups tolerates non-array", async () => {
    get.mockResolvedValue({ data: null });
    expect(await listGroups()).toEqual([]);
  });
});

describe("applySyncPlayMessage", () => {
  it("tracks group lifecycle, queue and commands", () => {
    let s = emptySyncPlayState();
    s = applySyncPlayMessage(
      s,
      update("GroupJoined", {
        GroupId: "g",
        GroupName: "n",
        State: "Idle",
        Participants: ["a"],
        LastUpdatedAt: "",
      }),
    );
    expect(s.group?.GroupName).toBe("n");
    s = applySyncPlayMessage(s, update("UserJoined", "b"));
    expect(s.group?.Participants).toEqual(["a", "b"]);
    s = applySyncPlayMessage(s, update("UserLeft", "a"));
    expect(s.group?.Participants).toEqual(["b"]);
    s = applySyncPlayMessage(
      s,
      update("PlayQueue", {
        Playlist: [{ ItemId: "i", PlaylistItemId: "p" }],
        PlayingItemIndex: 0,
      }),
    );
    expect(s.queue).toHaveLength(1);
    expect(s.playingIndex).toBe(0);
    s = applySyncPlayMessage(s, update("StateUpdate", { State: "Playing" }));
    expect(s.group?.State).toBe("Playing");
    s = applySyncPlayMessage(s, {
      MessageType: "SyncPlayCommand",
      Data: { Command: "Seek", PositionTicks: 9 },
    });
    expect(s.pendingCommand?.PositionTicks).toBe(9);
    expect(s.lastEvent).toBe("Command:Seek");
    s = applySyncPlayMessage(s, update("GroupLeft", "g"));
    expect(s.group).toBeNull();
  });

  it("ignores unrelated or malformed messages", () => {
    const s = emptySyncPlayState();
    expect(applySyncPlayMessage(s, "{bad")).toBe(s);
    expect(applySyncPlayMessage(s, { MessageType: "KeepAlive" })).toBe(s);
    expect(applySyncPlayMessage(s, update("UserJoined", "x"))).toBe(s);
  });

  it("reads shuffle/repeat modes from PlayQueue and cycles repeat", () => {
    const s = applySyncPlayMessage(
      emptySyncPlayState(),
      update("PlayQueue", {
        Playlist: [],
        PlayingItemIndex: 0,
        ShuffleMode: "Shuffle",
        RepeatMode: "RepeatOne",
      }),
    );
    expect([s.shuffleMode, s.repeatMode]).toEqual(["Shuffle", "RepeatOne"]);
    const t = applySyncPlayMessage(s, update("PlayQueue", { Playlist: [], RepeatMode: "Bogus" }));
    expect([t.shuffleMode, t.repeatMode]).toEqual(["Sorted", "RepeatNone"]);
    expect(nextRepeatMode("RepeatNone")).toBe("RepeatAll");
    expect(nextRepeatMode("RepeatAll")).toBe("RepeatOne");
    expect(nextRepeatMode("RepeatOne")).toBe("RepeatNone");
  });

  it("msUntil computes delay", () => {
    expect(msUntil({ When: "1970-01-01T00:00:01.000Z" }, 500)).toBe(500);
    expect(msUntil({ When: "nope" })).toBe(0);
  });
});
