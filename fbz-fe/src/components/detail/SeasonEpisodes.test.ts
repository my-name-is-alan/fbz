// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import type { EpisodeSummary, SeasonSummary } from "@/service/modules/detail.ts";
import SeasonEpisodes from "./SeasonEpisodes.vue";

// 组件改为直接对接后端真实数据：mock detail.ts 的 fetchSeasons / fetchEpisodes，
// 不再依赖任何设计态 mock 目录。
const { fetchSeasons, fetchEpisodes } = vi.hoisted(() => ({
  fetchSeasons: vi.fn(),
  fetchEpisodes: vi.fn(),
}));

vi.mock("@/service/modules/detail.ts", () => ({
  fetchSeasons,
  fetchEpisodes,
}));

function season(overrides: Partial<SeasonSummary> & Pick<SeasonSummary, "id">): SeasonSummary {
  return {
    id: overrides.id,
    seasonNumber: overrides.seasonNumber ?? null,
    name: overrides.name ?? "季",
    year: overrides.year,
    episodeCount: overrides.episodeCount,
    overview: overrides.overview,
    poster: overrides.poster,
  };
}

function episode(overrides: Partial<EpisodeSummary> & Pick<EpisodeSummary, "id">): EpisodeSummary {
  return {
    id: overrides.id,
    episodeNumber: overrides.episodeNumber ?? null,
    seasonNumber: overrides.seasonNumber ?? null,
    name: overrides.name ?? "分集",
    runtimeSeconds: overrides.runtimeSeconds,
    premiereDate: overrides.premiereDate,
    overview: overrides.overview,
    poster: overrides.poster,
    played: overrides.played ?? false,
    progressPercent: overrides.progressPercent,
  };
}

function makeEpisodes(seasonNumber: number, count: number): EpisodeSummary[] {
  return Array.from({ length: count }, (_, i) =>
    episode({
      id: `s${seasonNumber}e${i + 1}`,
      seasonNumber,
      episodeNumber: i + 1,
      name: `第 ${i + 1} 集`,
      runtimeSeconds: 2400,
      premiereDate: "2024-01-01",
    }),
  );
}

beforeEach(() => {
  fetchSeasons.mockReset();
  fetchEpisodes.mockReset();
});

describe("SeasonEpisodes.vue", () => {
  it("renders nothing when the series has no seasons", async () => {
    fetchSeasons.mockResolvedValue([]);
    const wrapper = mount(SeasonEpisodes, {
      props: { seriesId: "series-1", showTitle: "测试剧集" },
    });
    await flushPromises();

    expect(wrapper.find(".seasons-section").exists()).toBe(false);
  });

  it("renders single-season flat layout with real episodes", async () => {
    fetchSeasons.mockResolvedValue([
      season({ id: "season-1", seasonNumber: 1, name: "第一季", episodeCount: 10 }),
    ]);
    fetchEpisodes.mockResolvedValue(makeEpisodes(1, 10));

    const wrapper = mount(SeasonEpisodes, {
      props: { seriesId: "series-1", showTitle: "测试剧集" },
    });
    await flushPromises();

    expect(wrapper.find(".single-season-scroller-container").exists()).toBe(true);
    expect(wrapper.find(".multi-seasons-container").exists()).toBe(false);
    const cards = wrapper.findAll(".single-season-scroller-container .episode-card");
    expect(cards).toHaveLength(10);
    // 真实时长（2400s = 40分钟），不再是合成值。
    expect(cards[0].text()).toContain("40分钟");
  });

  it("shows the season grid for a multi-season series without watch history", async () => {
    fetchSeasons.mockResolvedValue([
      season({ id: "season-1", seasonNumber: 1, name: "第一季", episodeCount: 10 }),
      season({ id: "season-2", seasonNumber: 2, name: "第二季", episodeCount: 60 }),
    ]);
    // 首次全集拉取（无 seasonId）：无进度 → 不定位历史季。
    fetchEpisodes.mockResolvedValue([...makeEpisodes(1, 10), ...makeEpisodes(2, 60)]);

    const wrapper = mount(SeasonEpisodes, {
      props: { seriesId: "series-1", showTitle: "测试剧集" },
    });
    await flushPromises();

    expect(wrapper.find(".seasons-grid-container").exists()).toBe(true);
    expect(wrapper.find(".episodes-container").exists()).toBe(false);
    const cards = wrapper.findAll(".season-card-item");
    expect(cards).toHaveLength(2);
    expect(cards[0].text()).toContain("第一季");
    expect(cards[0].text()).toContain("10 集");
  });

  it("navigates to the episode list when a season card is clicked", async () => {
    fetchSeasons.mockResolvedValue([
      season({ id: "season-1", seasonNumber: 1, name: "第一季", episodeCount: 10 }),
      season({ id: "season-2", seasonNumber: 2, name: "第二季", episodeCount: 60 }),
    ]);
    fetchEpisodes.mockImplementation(async (_seriesId: string, seasonId?: string) => {
      if (seasonId === "season-1") return makeEpisodes(1, 10);
      if (seasonId === "season-2") return makeEpisodes(2, 60);
      return [...makeEpisodes(1, 10), ...makeEpisodes(2, 60)];
    });

    const wrapper = mount(SeasonEpisodes, {
      props: { seriesId: "series-1", showTitle: "测试剧集" },
    });
    await flushPromises();

    await wrapper.findAll(".season-card-item")[0].trigger("click");
    await flushPromises();

    expect(wrapper.find(".seasons-grid-container").exists()).toBe(false);
    expect(wrapper.find(".episodes-container").exists()).toBe(true);
    expect(wrapper.find(".season-banner h3").text()).toBe("第一季");
  });

  it("jumps to the season and range of the continue-watching episode", async () => {
    fetchSeasons.mockResolvedValue([
      season({ id: "season-1", seasonNumber: 1, name: "第一季", episodeCount: 10 }),
      season({ id: "season-2", seasonNumber: 2, name: "第二季", episodeCount: 60 }),
    ]);
    const s2 = makeEpisodes(2, 60);
    // 第二季第 55 集有进度且未看完 → 命中「继续观看」，应落在 E51-E60 范围。
    s2[54] = { ...s2[54]!, progressPercent: 40, played: false };
    fetchEpisodes.mockImplementation(async (_seriesId: string, seasonId?: string) => {
      if (seasonId === "season-1") return makeEpisodes(1, 10);
      if (seasonId === "season-2") return s2;
      return [...makeEpisodes(1, 10), ...s2];
    });

    const wrapper = mount(SeasonEpisodes, {
      props: { seriesId: "series-1", showTitle: "测试剧集" },
    });
    await flushPromises();

    expect(wrapper.find(".episodes-container").exists()).toBe(true);
    expect(wrapper.find(".season-banner h3").text()).toBe("第二季");

    const rangeTabs = wrapper.findAll(".range-tab");
    expect(rangeTabs).toHaveLength(2);
    expect(rangeTabs[0].text()).toBe("E1-E50");
    expect(rangeTabs[1].text()).toBe("E51-E60");
    expect(rangeTabs[1].classes()).toContain("active");
    // 「继续观看」提示与按钮出现。
    expect(wrapper.find(".continue-play-btn").exists()).toBe(true);
  });

  it("emits playEpisode with the real episode and its season list", async () => {
    fetchSeasons.mockResolvedValue([
      season({ id: "season-1", seasonNumber: 1, name: "第一季", episodeCount: 3 }),
    ]);
    const eps = makeEpisodes(1, 3);
    fetchEpisodes.mockResolvedValue(eps);

    const wrapper = mount(SeasonEpisodes, {
      props: { seriesId: "series-1", showTitle: "测试剧集" },
    });
    await flushPromises();

    await wrapper.findAll(".episode-card")[0].trigger("click");
    const events = wrapper.emitted("playEpisode");
    expect(events).toBeTruthy();
    const [payload] = events![0] as [{ episode: EpisodeSummary; episodes: EpisodeSummary[] }];
    expect(payload.episode.id).toBe("s1e1");
    expect(payload.episodes).toHaveLength(3);
  });
});
