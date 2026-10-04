export interface EngineTrack {
  label: string;
  supported?: boolean;
  format?: string;
  mime?: string;
  track?: { codecId?: string; width?: number; height?: number };
}
export interface EngineInfo {
  duration: number;
  native?: boolean;
  nativeReason?: string;
  video: EngineTrack[];
  audio: EngineTrack[];
  subtitles: EngineTrack[];
}
export interface WebPlayer {
  info: EngineInfo | null;
  log: (message: string, level?: string) => void;
  onStalled: ((event: { position: number }) => void) | null;
  loadAny(inputs: (string | { url: string; size?: number })[]): Promise<EngineInfo>;
  load(
    input: string | { url: string; size?: number },
    options?: { allowNative?: boolean },
  ): Promise<EngineInfo>;
  play(options?: { videoIndex?: number; audioIndex?: number }): Promise<void>;
  dispose(): Promise<void>;
}
export interface SubtitleController {
  select(index: number): Promise<unknown>;
  destroy(): Promise<void>;
}
export async function loadWebPlayer() {
  const root = `${import.meta.env.BASE_URL}webplayer/`;
  const [engine, subtitles] = await Promise.all([
    import(/* @vite-ignore */ `${root}src/player.js`),
    import(/* @vite-ignore */ `${root}src/subs/index.js`),
  ]);
  return {
    Player: engine.Player as new (video: HTMLVideoElement) => WebPlayer,
    Subtitles: subtitles.Subtitles as new (
      player: WebPlayer,
      options: { vendor: string },
    ) => SubtitleController,
    vendor: `${root}vendor/`,
  };
}

export function withPlaybackDeadline<T>(
  promise: Promise<T>,
  milliseconds: number,
  signal?: AbortSignal,
): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => finish(reject, new Error("PLAYBACK_TIMEOUT")), milliseconds);
    function finish(callback: (value: any) => void, value: unknown) {
      clearTimeout(timer);
      signal?.removeEventListener("abort", abort);
      callback(value);
    }
    function abort() {
      finish(reject, new DOMException("Playback cancelled", "AbortError"));
    }
    if (signal?.aborted) {
      void promise.catch(() => {});
      abort();
      return;
    }
    signal?.addEventListener("abort", abort, { once: true });
    void promise.then(
      (value) => finish(resolve, value),
      (error) => finish(reject, error),
    );
  });
}
