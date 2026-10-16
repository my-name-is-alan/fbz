import { describe, expect, it } from "vitest";
import {
  channelLayout,
  dynamicRangeOf,
  formatBitrate,
  formatBytes,
  normalizeContainer,
  resolutionLabel,
  sourceBadges,
  sourceLabel,
  toMediaSourceInfo,
} from "./mediaInfo.ts";

describe("mediaInfo", () => {
  it("maps MediaSourceDto streams into video/audio/subtitle groups", () => {
    const info = toMediaSourceInfo({
      Id: "src-1",
      Name: "Interstellar.2014.2160p",
      Container: "matroska,webm",
      RunTimeTicks: 101_700_000_000,
      Size: 64_424_509_440,
      Bitrate: 58_000_000,
      DefaultAudioStreamIndex: 2,
      SupportsDirectPlay: true,
      MediaStreams: [
        {
          Index: 0,
          Type: "Video",
          Codec: "hevc",
          Profile: "Main 10",
          Width: 3840,
          Height: 1600,
          BitDepth: 10,
          DisplayTitle: "4K HEVC HDR10",
        },
        { Index: 1, Type: "Audio", Codec: "truehd", Channels: 8, Language: "eng" },
        { Index: 2, Type: "Audio", Codec: "ac3", Channels: 6, Language: "chi", IsDefault: true },
        { Index: 3, Type: "Subtitle", Codec: "subrip", Language: "chi", IsDefault: true },
        {
          Index: 4,
          Type: "Subtitle",
          Codec: "ass",
          Language: "eng",
          IsExternal: true,
          DeliveryMethod: "External",
          DeliveryUrl: "/Videos/x/Subtitles/4/Stream.ass",
        },
      ],
    });

    expect(info.container).toBe("mkv");
    expect(info.runtimeSeconds).toBe(10170);
    expect(info.video).toHaveLength(1);
    expect(info.video[0]!.resolution).toBe("4K");
    expect(info.video[0]!.dynamicRange).toBe("HDR10");
    expect(info.audio.map((a) => a.layout)).toEqual(["7.1", "5.1"]);
    expect(info.subtitles[1]!.isExternal).toBe(true);
    expect(info.subtitles[1]!.deliveryUrl).toBe("/Videos/x/Subtitles/4/Stream.ass");
    // 主音轨取 IsDefault（AC3 5.1），不是第一条。
    expect(sourceBadges(info)).toEqual(["4K", "HDR10", "HEVC", "AC3 5.1", "MKV"]);
    expect(sourceLabel(info, 0)).toBe("Interstellar.2014.2160p · 4K · HDR10 · MKV");
  });

  it("never invents HDR from bit depth alone", () => {
    expect(dynamicRangeOf({ Type: "Video", Codec: "hevc", BitDepth: 10 })).toBeUndefined();
    const info = toMediaSourceInfo({
      MediaStreams: [{ Type: "Video", Codec: "hevc", Height: 1080, BitDepth: 10 }],
    });
    expect(sourceBadges(info)).toEqual(["1080P", "10bit", "HEVC"]);
  });

  it("recognises Dolby Vision codec tags", () => {
    expect(dynamicRangeOf({ Type: "Video", Codec: "hevc", CodecTag: "dvh1" })).toBe("Dolby Vision");
  });

  it("hides internal ids as version names", () => {
    const info = toMediaSourceInfo({ Id: "9", Name: "12345", Container: "mp4" }, 1);
    expect(sourceLabel(info, 1)).toBe("MP4");
    expect(sourceLabel(toMediaSourceInfo({ Name: "12345" }), 2)).toBe("版本 3");
  });

  it("formats sizes, bitrates and labels", () => {
    expect(resolutionLabel(720, 1280)).toBe("720P");
    expect(resolutionLabel(undefined, undefined)).toBeUndefined();
    expect(channelLayout(6)).toBe("5.1");
    expect(channelLayout(undefined)).toBeUndefined();
    expect(formatBytes(1536)).toBe("1.50 KB");
    expect(formatBytes(0)).toBeUndefined();
    expect(formatBitrate(58_000_000)).toBe("58.0 Mbps");
    expect(formatBitrate(640_000)).toBe("640 kbps");
    expect(normalizeContainer("mov,mp4,m4a")).toBe("mp4");
  });
});
