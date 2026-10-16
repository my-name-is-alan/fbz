import { describe, expect, it, vi } from "vitest";

vi.mock("@/service/request.ts", () => ({
  embyRequest: { get: vi.fn(), post: vi.fn() },
  mediaImageUrl: (p?: string | null) => p ?? undefined,
}));

import { formatRuntime } from "@/service/modules/detail.ts";

describe("formatRuntime", () => {
  it("shows seconds for clips under a minute instead of 0m", () => {
    expect(formatRuntime(12)).toBe("12s");
    expect(formatRuntime(0.4)).toBe("1s");
    expect(formatRuntime(undefined)).toBe("");
    expect(formatRuntime(48 * 60)).toBe("48m");
    expect(formatRuntime(126 * 60)).toBe("2h 6m");
  });
});
