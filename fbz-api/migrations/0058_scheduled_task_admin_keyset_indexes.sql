create index if not exists idx_scheduled_tasks_admin_keyset
    on scheduled_tasks (enabled desc, next_run_at asc nulls last, updated_at desc, id desc);

create index if not exists idx_scheduled_tasks_task_type_admin_keyset
    on scheduled_tasks (task_type, enabled desc, next_run_at asc nulls last, updated_at desc, id desc);

create index if not exists idx_scheduled_tasks_owner_type_admin_keyset
    on scheduled_tasks (owner_type, enabled desc, next_run_at asc nulls last, updated_at desc, id desc);
