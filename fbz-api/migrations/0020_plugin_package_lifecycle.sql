alter table plugin_packages
    add column if not exists permission_fingerprint bytea;

update plugin_packages
set permission_fingerprint = decode(repeat('00', 32), 'hex')
where permission_fingerprint is null;

alter table plugin_packages
    alter column permission_fingerprint set not null;

do $$
begin
    if not exists (
        select 1
        from pg_constraint
        where conname = 'plugin_packages_permission_fingerprint_len'
          and conrelid = 'plugin_packages'::regclass
    ) then
        alter table plugin_packages
            add constraint plugin_packages_permission_fingerprint_len
            check (length(permission_fingerprint) = 32);
    end if;
end
$$;
