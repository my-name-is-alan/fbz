import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

// audioStreamUrl 读 localStorage/token，mock 成纯函数：有 id 回固定形状，便于断言 streamUrl。
vi.mock("@/service/request.ts", () => ({
  audioStreamUrl: (id: string) => `STREAM(${id})`,
}));

import { useMusicPlayerStore } from "@/stores/musicPlayer.ts";
import type { PlayerTrack } from "@/stores/musicPlayer.ts";

const tracks: PlayerTrack[] = [
  { id: "t1", title: "One" },
  { id: "t2", title: "Two" },
  { id: "t3", title: "Three" },
];

beforeEach(() => {
  setActivePinia(createPinia());
});

describe("useMusicPlayerStore", () => {
  it("starts inactive with an empty queue", () => {
    const player = useMusicPlayerStore();
    expect(player.isActive).toBe(false);
    expect(player.current).toBeUndefined();
    expect(player.streamUrl).toBeUndefined();
    expect(player.hasPrevious).toBe(false);
    expect(player.hasNext).toBe(false);
  });

  it("plays a queue from the given start index", () => {
    const player = useMusicPlayerStore();
    player.playQueue(tracks, 1);
    expect(player.current?.id).toBe("t2");
    expect(player.streamUrl).toBe("STREAM(t2)");
    expect(player.hasPrevious).toBe(true);
    expect(player.hasNext).toBe(true);
  });

  it("clamps an out-of-range start index into the queue", () => {
    const player = useMusicPlayerStore();
    player.playQueue(tracks, 99);
    expect(player.current?.id).toBe("t3");
    expect(player.hasNext).toBe(false);
  });

  it("ignores an empty track list", () => {
    const player = useMusicPlayerStore();
    player.playQueue([]);
    expect(player.isActive).toBe(false);
  });

  it("steps through previous/next and respects boundaries", () => {
    const player = useMusicPlayerStore();
    player.playQueue(tracks, 0);
    expect(player.hasPrevious).toBe(false);

    player.playPrevious(); // 已在队首：不动。
    expect(player.current?.id).toBe("t1");

    player.playNext();
    expect(player.current?.id).toBe("t2");
    player.playNext();
    expect(player.current?.id).toBe("t3");

    player.playNext(); // 已在队尾：不动。
    expect(player.current?.id).toBe("t3");

    player.playPrevious();
    expect(player.current?.id).toBe("t2");
  });

  it("close clears the queue", () => {
    const player = useMusicPlayerStore();
    player.playQueue(tracks, 0);
    player.close();
    expect(player.isActive).toBe(false);
    expect(player.queue).toHaveLength(0);
    expect(player.currentIndex).toBe(-1);
  });
});
