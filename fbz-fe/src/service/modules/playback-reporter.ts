import { serverRequest, deviceId } from "./server.ts";

export interface PlaybackReport {
  ItemId: string;
  MediaSourceId: string;
  PlaySessionId: string;
  PositionTicks: number;
  IsPaused: boolean;
  PlayMethod: string;
  DeviceId?: string;
}
export function createPlaybackReporter(
  base: Omit<PlaybackReport, "PositionTicks" | "IsPaused">,
  send = (path: string, body: PlaybackReport) => serverRequest(path, body),
) {
  let started = false;
  let stopped = false;
  let queue = Promise.resolve();
  function report(kind: "start" | "progress" | "stop", seconds: number, paused: boolean) {
    if (stopped || (kind !== "start" && !started)) return queue;
    if (kind === "start" && started) kind = "progress";
    started = true;
    if (kind === "stop") stopped = true;
    const suffix = kind === "start" ? "" : kind === "stop" ? "/Stopped" : "/Progress";
    const body = {
      ...base,
      DeviceId: base.DeviceId ?? deviceId(),
      PositionTicks: Math.max(0, Math.round((Number.isFinite(seconds) ? seconds : 0) * 1e7)),
      IsPaused: paused,
    };
    const next = queue
      .then(() => send(`/emby/Sessions/Playing${suffix}`, body))
      .then(() => undefined);
    queue = next.catch(() => undefined);
    return next;
  }
  return { report };
}
