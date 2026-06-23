create table if not exists notification_target_secrets (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    target_id bigint not null references notification_targets(id) on delete cascade,
    secret_key text not null,
    algorithm text not null,
    nonce bytea not null,
    ciphertext bytea not null,
    value_hash bytea not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (public_id),
    unique (target_id, secret_key),
    check (length(trim(secret_key)) between 1 and 128),
    check (secret_key !~ '\\s'),
    check (algorithm in ('xchacha20poly1305-sha256-key-v1')),
    check (length(nonce) = 24),
    check (length(ciphertext) > 0),
    check (length(value_hash) = 32)
);

create index if not exists idx_notification_target_secrets_target
    on notification_target_secrets (target_id, secret_key);
