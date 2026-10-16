import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/service/request.ts", () => ({
  embyRequest: { get: vi.fn(), post: vi.fn(), delete: vi.fn() },
  mediaImageUrl: (p?: string | null) => p ?? undefined,
}));

import { embyRequest } from "@/service/request.ts";
import {
  fetchPlaylistSharing,
  fetchShareableUsers,
  setPlaylistPublic,
  sharePlaylist,
  unsharePlaylist,
} from "@/service/modules/playlists.ts";

const get = vi.mocked(embyRequest.get);
const post = vi.mocked(embyRequest.post);
const del = vi.mocked(embyRequest.delete);

beforeEach(() => {
  get.mockReset();
  post.mockReset();
  del.mockReset();
});

describe("playlist sharing", () => {
  it("reads PlaylistDto sharing shape", async () => {
    get.mockResolvedValue({
      data: { OpenAccess: true, Shares: [{ UserId: "u2", CanEdit: false }], ItemIds: [] },
    });
    expect(await fetchPlaylistSharing("p")).toEqual({
      openAccess: true,
      shares: [{ UserId: "u2", CanEdit: false }],
    });
    get.mockResolvedValue({ data: {} });
    expect(await fetchPlaylistSharing("p")).toEqual({ openAccess: false, shares: [] });
  });

  it("uses JF endpoints for public / share / unshare", async () => {
    await setPlaylistPublic("p", true);
    expect(post).toHaveBeenCalledWith("/Playlists/p", null, { params: { IsPublic: true } });
    await sharePlaylist("p", "u2", true);
    expect(post).toHaveBeenCalledWith("/Playlists/p/Users/u2", { CanEdit: true });
    await unsharePlaylist("p", "u2");
    expect(del).toHaveBeenCalledWith("/Playlists/p/Users/u2");
  });

  it("lists shareable users excluding self", async () => {
    get.mockResolvedValue({
      data: [
        { Id: "me", Name: "Me" },
        { Id: "u2", Name: "Bob" },
      ],
    });
    expect(await fetchShareableUsers("me")).toEqual([{ id: "u2", name: "Bob" }]);
  });
});
