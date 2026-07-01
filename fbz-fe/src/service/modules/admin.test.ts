import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock the shared axios instance so tests assert call shape without network.
vi.mock("@/service/request.ts", () => {
  return {
    request: {
      get: vi.fn(),
      post: vi.fn(),
      delete: vi.fn(),
    },
  };
});

import { request } from "@/service/request.ts";
import {
  addLibraryPath,
  createLibrary,
  deleteLibrary,
  fetchPhotoThumbnail,
  getMetadataSettings,
  listLibraries,
  listLibraryPaths,
  listLibraryPhotos,
  removeLibraryPath,
  setLibraryPathEnabled,
  testMetadataProvider,
  updateLibrarySettings,
} from "./admin.ts";

const mockGet = vi.mocked(request.get);
const mockPost = vi.mocked(request.post);
const mockDelete = vi.mocked(request.delete);

beforeEach(() => {
  vi.clearAllMocks();
});

describe("admin service – libraries", () => {
  it("createLibrary posts to /admin/libraries and returns the record", async () => {
    mockPost.mockResolvedValueOnce({
      data: { id: "lib-1", name: "Movies", libraryType: "movies" },
    });
    const result = await createLibrary({ name: "Movies", libraryType: "movies" });
    expect(mockPost).toHaveBeenCalledWith("/admin/libraries", {
      name: "Movies",
      libraryType: "movies",
    });
    expect(result.id).toBe("lib-1");
  });

  it("listLibraries normalizes keyset pagination headers", async () => {
    mockGet.mockResolvedValueOnce({
      data: [{ id: "lib-1", name: "Movies", libraryType: "movies", isHidden: false }],
      headers: { "x-fbz-has-more": "true", "x-fbz-next-cursor": "cursor-2" },
    });
    const page = await listLibraries({ libraryType: "movies", limit: 1 });
    expect(mockGet).toHaveBeenCalledWith("/admin/libraries", {
      params: { libraryType: "movies", limit: 1 },
    });
    expect(page.items).toHaveLength(1);
    expect(page.hasMore).toBe(true);
    expect(page.nextCursor).toBe("cursor-2");
  });

  it("listLibraries reports no further page when header absent", async () => {
    mockGet.mockResolvedValueOnce({ data: [], headers: {} });
    const page = await listLibraries();
    expect(page.hasMore).toBe(false);
    expect(page.nextCursor).toBeNull();
  });

  it("updateLibrarySettings posts to the settings sub-path", async () => {
    mockPost.mockResolvedValueOnce({
      data: { id: "lib-1", name: "Movies", libraryType: "movies", isHidden: true },
    });
    await updateLibrarySettings("lib-1", { isHidden: true });
    expect(mockPost).toHaveBeenCalledWith("/admin/libraries/lib-1/settings", { isHidden: true });
  });

  it("deleteLibrary issues a DELETE on the library", async () => {
    mockDelete.mockResolvedValueOnce({ data: undefined });
    await deleteLibrary("lib-1");
    expect(mockDelete).toHaveBeenCalledWith("/admin/libraries/lib-1");
  });

  it("encodes ids that contain reserved characters", async () => {
    mockDelete.mockResolvedValueOnce({ data: undefined });
    await deleteLibrary("a/b");
    expect(mockDelete).toHaveBeenCalledWith("/admin/libraries/a%2Fb");
  });
});

describe("admin service – library paths", () => {
  it("addLibraryPath posts the path body", async () => {
    mockPost.mockResolvedValueOnce({
      data: { id: "1", libraryId: "lib-1", path: "/media", isEnabled: true },
    });
    await addLibraryPath("lib-1", "/media");
    expect(mockPost).toHaveBeenCalledWith("/admin/libraries/lib-1/paths", { path: "/media" });
  });

  it("listLibraryPaths gets the paths sub-path", async () => {
    mockGet.mockResolvedValueOnce({ data: [], headers: {} });
    await listLibraryPaths("lib-1");
    expect(mockGet).toHaveBeenCalledWith("/admin/libraries/lib-1/paths");
  });

  it("removeLibraryPath deletes the scoped path", async () => {
    mockDelete.mockResolvedValueOnce({ data: undefined });
    await removeLibraryPath("lib-1", "42");
    expect(mockDelete).toHaveBeenCalledWith("/admin/libraries/lib-1/paths/42");
  });

  it("setLibraryPathEnabled posts the toggle to the settings sub-path", async () => {
    mockPost.mockResolvedValueOnce({
      data: { id: "42", libraryId: "lib-1", path: "/media", isEnabled: false },
    });
    await setLibraryPathEnabled("lib-1", "42", false);
    expect(mockPost).toHaveBeenCalledWith("/admin/libraries/lib-1/paths/42/settings", {
      isEnabled: false,
    });
  });
});

describe("admin service – photos", () => {
  it("listLibraryPhotos normalizes keyset pagination headers", async () => {
    mockGet.mockResolvedValueOnce({
      data: [{ id: "photo-1", title: "IMG_0001", hasThumbnail: true }],
      headers: { "x-fbz-has-more": "true", "x-fbz-next-cursor": "photo-cursor-2" },
    });
    const page = await listLibraryPhotos("lib-1", { limit: 1 });
    expect(mockGet).toHaveBeenCalledWith("/admin/libraries/lib-1/photos", {
      params: { limit: 1 },
    });
    expect(page.items).toHaveLength(1);
    expect(page.hasMore).toBe(true);
    expect(page.nextCursor).toBe("photo-cursor-2");
  });

  it("fetchPhotoThumbnail requests a blob and returns an object URL", async () => {
    const blob = new Blob(["x"], { type: "image/jpeg" });
    mockGet.mockResolvedValueOnce({ data: blob, headers: {} });
    const createObjectURL = vi.fn(() => "blob:fake-url");
    vi.stubGlobal("URL", { createObjectURL });

    const url = await fetchPhotoThumbnail("photo-1");
    expect(mockGet).toHaveBeenCalledWith("/admin/media-items/photo-1/thumbnail", {
      responseType: "blob",
    });
    expect(createObjectURL).toHaveBeenCalledWith(blob);
    expect(url).toBe("blob:fake-url");

    vi.unstubAllGlobals();
  });
});

describe("admin service – metadata", () => {
  it("getMetadataSettings reads the settings endpoint", async () => {
    mockGet.mockResolvedValueOnce({ data: { global: {}, providers: [] }, headers: {} });
    await getMetadataSettings();
    expect(mockGet).toHaveBeenCalledWith("/admin/metadata/settings");
  });

  it("testMetadataProvider posts to the provider test endpoint", async () => {
    mockPost.mockResolvedValueOnce({ data: { provider: "tmdb", ok: true, message: "ok" } });
    const result = await testMetadataProvider("tmdb");
    expect(mockPost).toHaveBeenCalledWith("/admin/metadata/providers/tmdb/test");
    expect(result.ok).toBe(true);
  });
});
