/**
 * 播放进度上报 service：对接 Emby 兼容面的 `POST /Sessions/Playing*`。
 *
 * 走 {@link embyRequest}（根面，自动带 `x-emby-token`），让后端把进度写入 `user_playstates`
 * （见 `compat/emby/routes/playback.rs`），从而让音乐曲目出现在「继续观看」（resume 查询不限
 * item_type，带 `position_ticks>0` 即纳入）。三事件：开始 / 进度 / 停止。
 *
 * 位置单位：Emby 用 tick（100ns），秒 × 10_000_000；本模块对外收秒、内部转 tick。
 */
import { embyRequest } from "@/service/request.ts";

/** 1 秒对应的 Emby tick 数（100ns/tick）。 */
const TICKS_PER_SECOND = 10_000_000;

function toTicks(seconds: number): number {
  return Math.max(0, Math.round(seconds * TICKS_PER_SECOND));
}

/** 上报公共入参（音乐用 DirectStream 直出）。 */
interface ReportArgs {
  itemId: string;
  /** 客户端会话标识，贯穿一次播放的开始→进度→停止，便于后端关联。 */
  playSessionId: string;
  /** 当前播放位置（秒）。 */
  positionSeconds: number;
  isPaused?: boolean;
}

/** 播放开始：`POST /Sessions/Playing`。 */
export async function reportPlaybackStart(args: ReportArgs): Promise<void> {
  await embyRequest.post("/Sessions/Playing", {
    ItemId: args.itemId,
    PlaySessionId: args.playSessionId,
    PlayMethod: "DirectStream",
    PositionTicks: toTicks(args.positionSeconds),
    IsPaused: args.isPaused ?? false,
  });
}

/** 播放进度：`POST /Sessions/Playing/Progress`（节流后周期上报）。 */
export async function reportPlaybackProgress(args: ReportArgs): Promise<void> {
  await embyRequest.post("/Sessions/Playing/Progress", {
    ItemId: args.itemId,
    PlaySessionId: args.playSessionId,
    PlayMethod: "DirectStream",
    EventName: "TimeUpdate",
    PositionTicks: toTicks(args.positionSeconds),
    IsPaused: args.isPaused ?? false,
  });
}

/** 播放停止：`POST /Sessions/Playing/Stopped`（切歌/关闭/播完时落最终位置）。 */
export async function reportPlaybackStopped(args: ReportArgs): Promise<void> {
  await embyRequest.post("/Sessions/Playing/Stopped", {
    ItemId: args.itemId,
    PlaySessionId: args.playSessionId,
    PlayMethod: "DirectStream",
    PositionTicks: toTicks(args.positionSeconds),
  });
}

/** 生成一次播放会话的客户端标识（开始→停止贯穿同一值）。 */
export function newPlaySessionId(): string {
  return `fbz-web-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}
