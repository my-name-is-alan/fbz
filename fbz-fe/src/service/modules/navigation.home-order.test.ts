// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  applyHomeSectionOrder,
  mapHomeSectionIdToRowKey,
  normalizeHomeSectionId,
  parseHomesectionPrefs,
} from "./navigation.ts";
import type { HomeRow } from "@/types/navigation.ts";

function row(key: string): HomeRow {
  return { key, title: key, layout: "poster", items: [] };
}

describe("home section preference ordering", () => {
  it("normalizes and maps Emby section ids to row keys", () => {
    expect(normalizeHomeSectionId("Continue_Watching")).toBe("continuewatching");
    expect(mapHomeSectionIdToRowKey("resume")).toBe("continue");
    expect(mapHomeSectionIdToRowKey("latestmedia")).toBe("latest");
    expect(mapHomeSectionIdToRowKey("favourites")).toBe("favorites");
    expect(mapHomeSectionIdToRowKey("movies")).toBeNull();
  });

  it("parses homesectionN prefs and skips none/blank", () => {
    expect(
      parseHomesectionPrefs({
        homesection0: "resume",
        homesection1: "none",
        homesection2: "favorites",
        homesection3: "latestmedia",
      }),
    ).toEqual(["resume", "favorites", "latestmedia"]);
    expect(parseHomesectionPrefs({})).toEqual([]);
    expect(parseHomesectionPrefs(undefined)).toEqual([]);
  });

  it("reorders rows by preference and appends leftovers", () => {
    const rows = [row("continue"), row("favorites"), row("latest")];
    expect(applyHomeSectionOrder(rows, ["latest", "continue"]).map((r) => r.key)).toEqual([
      "latest",
      "continue",
      "favorites",
    ]);
    expect(applyHomeSectionOrder(rows, []).map((r) => r.key)).toEqual([
      "continue",
      "favorites",
      "latest",
    ]);
  });
});
