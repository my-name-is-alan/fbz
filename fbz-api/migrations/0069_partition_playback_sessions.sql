-- Partition playback_sessions by started_at (monthly RANGE).
--
-- Category C from docs/database-partitioning-design.md, the harder variant: the
-- inbound FK transcoding_sessions.playback_session_id -> playback_sessions(id)
-- ON DELETE SET NULL has an *exercised* path (playback_sessions are deleted
-- independently via users/media_items cascade while a transcoding_session may
-- survive). A plain drop would leave dangling references, so the SET NULL
-- behavior is replicated by a BEFORE DELETE trigger before the FK is dropped.
-- public_id UNIQUE is relaxed to a plain index (random uuid; inserts are plain,
-- no ON CONFLICT). Structural change, confirmed.

-- 1. Replicate the inbound SET NULL via a trigger, then drop the FK.
create index if not exists idx_transcoding_sessions_playback_session
    on transcoding_sessions (playback_session_id)
    where playback_session_id is not null;

create or replace function playback_sessions_null_transcode_refs()
returns trigger
language plpgsql
as $$
begin
    update transcoding_sessions
    set playback_session_id = null
    where playback_session_id = old.id;
    return old;
end
$$;

alter table transcoding_sessions drop constraint transcoding_sessions_playback_session_id_fkey;

-- 2. Partition playback_sessions.
alter sequence playback_sessions_id_seq owned by none;

alter table playback_sessions rename to playback_sessions_legacy;
alter table playback_sessions_legacy rename constraint playback_sessions_pkey to playback_sessions_legacy_pkey;
alter table playback_sessions_legacy rename constraint playback_sessions_check to playback_sessions_legacy_check;
alter table playback_sessions_legacy rename constraint playback_sessions_play_method_check to playback_sessions_legacy_play_method_check;
alter table playback_sessions_legacy rename constraint playback_sessions_position_ticks_check to playback_sessions_legacy_position_ticks_check;
alter table playback_sessions_legacy rename constraint playback_sessions_device_id_fkey to playback_sessions_legacy_device_id_fkey;
alter table playback_sessions_legacy rename constraint playback_sessions_media_file_id_fkey to playback_sessions_legacy_media_file_id_fkey;
alter table playback_sessions_legacy rename constraint playback_sessions_media_item_id_fkey to playback_sessions_legacy_media_item_id_fkey;
alter table playback_sessions_legacy rename constraint playback_sessions_user_id_fkey to playback_sessions_legacy_user_id_fkey;
alter index playback_sessions_public_id_key rename to playback_sessions_legacy_public_id_key;
alter index idx_playback_sessions_item_active rename to idx_playback_sessions_legacy_item_active;
alter index idx_playback_sessions_user_active rename to idx_playback_sessions_legacy_user_active;

create table playback_sessions (
    id bigint not null default nextval('playback_sessions_id_seq'),
    public_id uuid not null default gen_random_uuid(),
    user_id bigint not null references users(id) on delete cascade,
    device_id bigint references devices(id) on delete set null,
    media_item_id bigint not null references media_items(id) on delete cascade,
    media_file_id bigint references media_files(id) on delete set null,
    play_method text not null,
    position_ticks bigint not null default 0,
    is_paused boolean not null default false,
    started_at timestamptz not null default now(),
    last_progress_at timestamptz not null default now(),
    stopped_at timestamptz,
    client_session_id text,
    remote_addr inet,
    primary key (id, started_at),
    check (stopped_at is null or stopped_at >= started_at),
    check (play_method in ('direct_play', 'direct_stream', 'transcode', 'strm_redirect')),
    check (position_ticks >= 0)
) partition by range (started_at);

alter sequence playback_sessions_id_seq owned by playback_sessions.id;

create table playback_sessions_2026m06 partition of playback_sessions
    for values from ('2026-06-01') to ('2026-07-01');
create table playback_sessions_2026m07 partition of playback_sessions
    for values from ('2026-07-01') to ('2026-08-01');
create table playback_sessions_2026m08 partition of playback_sessions
    for values from ('2026-08-01') to ('2026-09-01');
create table playback_sessions_default partition of playback_sessions default;

-- public_id relaxed to a plain index (partition key cannot be in a global unique here).
create index idx_playback_sessions_public_id on playback_sessions (public_id);
create index idx_playback_sessions_item_active
    on playback_sessions (media_item_id, stopped_at, last_progress_at desc);
create index idx_playback_sessions_user_active
    on playback_sessions (user_id, stopped_at, last_progress_at desc);

insert into playback_sessions (
    id, public_id, user_id, device_id, media_item_id, media_file_id, play_method,
    position_ticks, is_paused, started_at, last_progress_at, stopped_at, client_session_id, remote_addr
)
select id, public_id, user_id, device_id, media_item_id, media_file_id, play_method,
    position_ticks, is_paused, started_at, last_progress_at, stopped_at, client_session_id, remote_addr
from playback_sessions_legacy;

select setval(
    'playback_sessions_id_seq',
    (select coalesce(max(id), 0) from playback_sessions) + 1,
    false
);

drop table playback_sessions_legacy;

-- 3. Attach the SET NULL replicating trigger to the new partitioned table.
create trigger trg_playback_sessions_null_transcode_refs
before delete on playback_sessions
for each row execute function playback_sessions_null_transcode_refs();

-- 4. Add playback_sessions to the rolling coverage function and extend forward.
create or replace function ensure_partition_coverage(months_ahead int)
returns int
language plpgsql
as $$
declare
    tbl text;
    i int;
    start_bound date;
    end_bound date;
    part_name text;
    created int := 0;
begin
    foreach tbl in array array[
        'job_events', 'plugin_host_api_calls', 'scheduled_task_runs', 'job_runs',
        'playback_sessions'
    ]
    loop
        for i in 0..greatest(months_ahead, 0) loop
            start_bound := (date_trunc('month', now()) + make_interval(months => i))::date;
            end_bound := (start_bound + interval '1 month')::date;
            part_name := tbl || '_' || to_char(start_bound, 'YYYY"m"MM');
            if not exists (select 1 from pg_class where relname = part_name) then
                execute format(
                    'create table %I partition of %I for values from (%L) to (%L)',
                    part_name, tbl, start_bound, end_bound
                );
                created := created + 1;
            end if;
        end loop;
    end loop;
    return created;
end
$$;

select ensure_partition_coverage(18);
