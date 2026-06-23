create index if not exists idx_libraries_admin_name_keyset
    on libraries (name asc, id asc);

create index if not exists idx_libraries_admin_type_name_keyset
    on libraries (library_type, name asc, id asc);
