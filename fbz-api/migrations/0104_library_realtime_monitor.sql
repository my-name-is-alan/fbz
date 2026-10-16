-- 0104_library_realtime_monitor.sql（additive）
-- Library Monitor：库级 EnableRealtimeMonitor 开关。默认 false；开启后由
-- scan/monitor 轮询（或日后 notify）路径变更并入队 library.scan。

alter table libraries
    add column if not exists enable_realtime_monitor boolean not null default false;

comment on column libraries.enable_realtime_monitor is
    'Emby/JF LibraryOptions.EnableRealtimeMonitor; when true, Library Monitor may enqueue library.scan on path changes';
