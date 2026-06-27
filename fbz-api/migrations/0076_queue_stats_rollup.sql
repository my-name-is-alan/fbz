create table if not exists queue_stats_rollup (
    bucket_date date not null,
    table_name text not null,
    status text not null,
    row_count bigint not null default 0,
    finalized_at timestamptz not null default now(),
    source_partition text,
    primary key (bucket_date, table_name, status),
    check (length(trim(table_name)) > 0),
    check (length(trim(status)) > 0),
    check (row_count >= 0),
    check (source_partition is null or length(trim(source_partition)) > 0)
);

create index if not exists idx_queue_stats_rollup_table_bucket
    on queue_stats_rollup (table_name, bucket_date desc, status);
