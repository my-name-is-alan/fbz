# Library Monitor（路径变更 → 增量扫描）最小落地

状态：**可验收最小实现**（轮询 + 库级开关 + 入队既有 `library.scan`）。未引入 `notify` 依赖；`NotifyStubBackend` 仅占位。

| 项 | 落点 |
| --- | --- |
| 库级开关 | `libraries.enable_realtime_monitor`（迁移 `0104`）；admin GET/PUT；VirtualFolder `LibraryOptions.EnableRealtimeMonitor` |
| Worker | `scan/monitor.rs` + `FBZ_LIBRARY_MONITOR_*`；`main` 与 readiness `library_monitor` |
| 入队 | `AdminRepository::queue_library_scan`（reason=`realtime_monitor`；既有 queued/running 去重） |
| SystemInfo | `SupportsLibraryMonitor` ← `config.library_monitor.enabled` |
| 指纹 | 根目录 + 一层子项 mtime/size（浅层）；debounce 默认 60s |
| 排除 | `storage_mounts` 挂载库不监视（云盘走既有 refresh） |

## 启用

1. `FBZ_LIBRARY_MONITOR_ENABLED=true`（且 node role 含 worker）
2. 对该库 `enableRealtimeMonitor: true`（admin 库设置）
3. 扫描 worker 仍需开启以真正执行 `library.scan`

## 后续（非本轮）

- `notify` / FSEvents 替换轮询（实现 `NotifyBackend`）
- 局部 Discover（仅变更路径）而非整库 scan
- 与 scheduler 增量策略的冲突退避
