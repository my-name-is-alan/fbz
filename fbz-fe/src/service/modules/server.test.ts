// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { authenticate, listItems, preparePlayback, readSession, saveSession } from "./server.ts";
const mocks = vi.hoisted(() => ({ request: vi.fn(), post: vi.fn() }));
vi.mock("@/service/request.ts", () => ({
  request: mocks,
  getAccessToken: () =>
    sessionStorage.getItem("fbz_access_token") ?? localStorage.getItem("fbz_access_token"),
  setAccessToken: (token: string | null) =>
    token
      ? localStorage.setItem("fbz_access_token", token)
      : localStorage.removeItem("fbz_access_token"),
}));
beforeEach(() => {
  localStorage.clear();
  sessionStorage.clear();
  vi.resetAllMocks();
});
describe("server integration", () => {
  it("stores real authentication in session storage unless remembered", async () => {
    mocks.post.mockResolvedValue({
      data: { AccessToken: "test-token", User: { Id: "u1", Name: "Tester" } },
    });
    await authenticate("http://localhost:8080", "Tester", "test-only-password");
    expect(readSession()?.userId).toBe("u1");
    expect(localStorage.getItem("fbz_session")).toBeNull();
    expect(mocks.post).toHaveBeenCalledWith(
      "http://localhost:8080/emby/Users/AuthenticateByName",
      { Username: "Tester", Pw: "test-only-password" },
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: expect.stringContaining('Emby Client="FBZ Web"'),
        }),
      }),
    );
  });
  it("lists only the current user's media through the supported route", async () => {
    saveSession({
      token: "test-token",
      userId: "u1",
      username: "Tester",
      address: "http://localhost:8080",
    });
    mocks.request.mockResolvedValue({ data: { Items: [], TotalRecordCount: 0 } });
    await listItems("library-1", 60, "test title");
    expect(mocks.request).toHaveBeenCalledWith(
      expect.objectContaining({
        baseURL: window.location.origin,
        url: expect.stringContaining("/emby/Users/u1/Items?"),
      }),
    );
    const options = mocks.request.mock.calls[0]?.[0];
    expect(options?.url).toContain("StartIndex=60");
    expect(options?.url).toContain("ParentId=library-1");
    expect(options?.url).toContain("EnableImages=true");
  });
  it("resolves the server stream and resumes from persisted progress", async () => {
    saveSession({
      token: "test-token",
      userId: "u1",
      username: "Tester",
      address: "http://localhost:8080",
    });
    mocks.request
      .mockResolvedValueOnce({ data: { Id: "m1", UserData: { PlaybackPositionTicks: 250000000 } } })
      .mockResolvedValueOnce({ data: [{ Id: "m1", Overview: "Metadata", ImageTags: {} }] })
      .mockResolvedValueOnce({
        data: {
          PlaySessionId: "p1",
          MediaSources: [
            { Id: "1", RunTimeTicks: 1200000000, DirectStreamUrl: "/emby/Videos/m1/stream" },
          ],
        },
      });
    const result = await preparePlayback({ Id: "m1", Name: "Test", Type: "Movie" });
    expect(result.source.uri).toBe(`${window.location.origin}/emby/Videos/m1/stream`);
    expect(result.server.startTicks).toBe(250000000);
    expect(result.duration).toBe(120);
  });
});
