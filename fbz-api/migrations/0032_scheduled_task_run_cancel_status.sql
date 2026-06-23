alter table scheduled_task_runs
    drop constraint if exists scheduled_task_runs_status_check;

alter table scheduled_task_runs
    add constraint scheduled_task_runs_status_check
    check (status in ('running', 'succeeded', 'failed', 'expired', 'cancelled'));
