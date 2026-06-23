// @vitest-environment jsdom
import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import SeasonEpisodes from "./SeasonEpisodes.vue";

// Mock tmdb module
vi.mock("@/service/modules/tmdb.ts", () => ({
  imageUrl: (path: string) => `mocked-url-${path}`,
}));

const mockSeasons = [
  {
    season_number: 1,
    name: "第一季",
    episode_count: 10,
    air_date: "2024-01-01",
    overview: "第一季的故事内容",
    poster_path: "/path1.jpg",
  },
  {
    season_number: 2,
    name: "第二季",
    episode_count: 60, // > 50 for range testing
    air_date: "2025-01-01",
    overview: "第二季的故事内容",
    poster_path: "/path2.jpg",
  },
];

describe("SeasonEpisodes.vue", () => {
  it("renders horizontal scroll layout directly if only a single season is provided", () => {
    const wrapper = mount(SeasonEpisodes, {
      props: {
        seasons: [mockSeasons[0]], // Only 1 season
        showTitle: "测试剧集",
      },
    });

    // Should display single season scroller container and not the multiple seasons views
    expect(wrapper.find(".single-season-scroller-container").exists()).toBe(true);
    expect(wrapper.find(".multi-seasons-container").exists()).toBe(false);

    // Should render episode cards inside scroller
    const cards = wrapper.findAll(".single-season-scroller-container .episode-card");
    expect(cards).toHaveLength(10); // Mock season 1 has 10 episodes
  });

  it("renders season grid when no history is provided for multi-season", () => {
    const wrapper = mount(SeasonEpisodes, {
      props: {
        seasons: mockSeasons,
        showTitle: "测试剧集",
      },
    });

    // Should display seasons grid and not the episode list
    expect(wrapper.find(".seasons-grid-container").exists()).toBe(true);
    expect(wrapper.find(".episodes-container").exists()).toBe(false);

    // Should render two season cards
    const cards = wrapper.findAll(".season-card-item");
    expect(cards).toHaveLength(2);
    expect(cards[0].text()).toContain("第一季");
    expect(cards[0].text()).toContain("10 集");
    expect(cards[1].text()).toContain("第二季");
    expect(cards[1].text()).toContain("60 集");
  });

  it("navigates to episode list when a season card is clicked for multi-season", async () => {
    const wrapper = mount(SeasonEpisodes, {
      props: {
        seasons: mockSeasons,
        showTitle: "测试剧集",
      },
    });

    // Click the first season card
    const cards = wrapper.findAll(".season-card-item");
    await cards[0].trigger("click");

    // Should transition to episodes view
    expect(wrapper.find(".seasons-grid-container").exists()).toBe(false);
    expect(wrapper.find(".episodes-container").exists()).toBe(true);
    expect(wrapper.find(".season-banner h3").text()).toBe("第一季");
  });

  it("directly navigates to episode list if defaultSeason (history) is provided for multi-season", () => {
    const wrapper = mount(SeasonEpisodes, {
      props: {
        seasons: mockSeasons,
        showTitle: "测试剧集",
        defaultSeason: 2,
        watchedEpisode: 55, // Should fall into E51-E60 range
      },
    });

    // Should show episodes view directly
    expect(wrapper.find(".seasons-grid-container").exists()).toBe(false);
    expect(wrapper.find(".episodes-container").exists()).toBe(true);
    expect(wrapper.find(".season-banner h3").text()).toBe("第二季");

    // Should calculate range tabs and highlight the second tab (E51-E60)
    const rangeTabs = wrapper.findAll(".range-tab");
    expect(rangeTabs).toHaveLength(2);
    expect(rangeTabs[0].text()).toBe("E1-E50");
    expect(rangeTabs[1].text()).toBe("E51-E60");
    expect(rangeTabs[1].classes()).toContain("active");
  });

  it("can return to season grid from episode view in multi-season", async () => {
    const wrapper = mount(SeasonEpisodes, {
      props: {
        seasons: mockSeasons,
        showTitle: "测试剧集",
        defaultSeason: 1,
      },
    });

    // Initially in episodes view
    expect(wrapper.find(".episodes-container").exists()).toBe(true);

    // Click back button
    const backBtn = wrapper.find(".back-to-seasons-btn");
    await backBtn.trigger("click");

    // Should go back to season list
    expect(wrapper.find(".seasons-grid-container").exists()).toBe(true);
    expect(wrapper.find(".episodes-container").exists()).toBe(false);
  });
});
