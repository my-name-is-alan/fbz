alter table plugin_host_api_calls
    drop constraint if exists plugin_host_api_calls_method_check;

alter table plugin_host_api_calls
    add constraint plugin_host_api_calls_method_check
    check (method in ('GET', 'POST', 'PUT', 'PATCH', 'DELETE')) not valid;

alter table plugin_host_api_calls
    validate constraint plugin_host_api_calls_method_check;
