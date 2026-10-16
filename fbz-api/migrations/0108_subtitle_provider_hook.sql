-- Adds `subtitle.provider.query` (remote subtitle provider plugins, e.g. OpenSubtitles)
-- to the plugin_hooks.event_key allow-list. Additive; mirrors 0102.
alter table plugin_hooks drop constraint if exists plugin_hooks_event_key_allowed;
alter table plugin_hooks
    add constraint plugin_hooks_event_key_allowed
    check (event_key in (
        'library.scan.started',
        'library.scan.completed',
        'library.scan.failed',
        'media.item.created',
        'media.item.updated',
        'media.download.started',
        'metadata.refresh.completed',
        'metadata.refresh.failed',
        'metadata.provider.query',
        'media.marker.detect',
        'subtitle.provider.query',
        'playback.started',
        'playback.stopped',
        'scheduler.tick',
        'transcode.started',
        'transcode.completed',
        'transcode.failed',
        'user.login',
        'webhook.received'
    ));
