do $$
declare
    constraint_name text;
begin
    for constraint_name in
        select con.conname
        from pg_constraint con
        where con.conrelid = 'plugin_hooks'::regclass
          and con.contype = 'c'
          and pg_get_constraintdef(con.oid) like '%event_key%'
          and pg_get_constraintdef(con.oid) like '%library.scan.started%'
    loop
        execute format('alter table plugin_hooks drop constraint %I', constraint_name);
    end loop;
end
$$;

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
        'playback.started',
        'playback.stopped',
        'scheduler.tick',
        'transcode.started',
        'transcode.completed',
        'transcode.failed',
        'user.login',
        'webhook.received'
    ));
