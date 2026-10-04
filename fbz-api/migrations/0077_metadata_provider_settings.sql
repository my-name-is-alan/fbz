-- Metadata scraper configuration: admin-managed provider settings + global
-- defaults + encrypted API keys. Purely additive: three new tables, no changes
-- to existing schema. Runtime config = environment-variable defaults overlaid
-- with these rows when present (see docs/plans/metadata-scraper-design.md §5).

-- Global metadata defaults. Enforced single row via id check so the runtime can
-- always read/upsert a canonical row without scanning.
create table if not exists metadata_global_settings (
    id smallint primary key default 1,
    provider_order text[] not null default '{}',
    default_language text,
    default_country text,
    image_language text,
    image_prefer_original boolean not null default false,
    image_fallback_languages text[] not null default '{}',
    updated_at timestamptz not null default now(),
    check (id = 1),
    check (default_language is null or length(trim(default_language)) between 1 and 16),
    check (default_country is null or default_country ~ '^[A-Z]{2}$'),
    check (image_language is null or length(trim(image_language)) between 1 and 16)
);

-- Per-provider configuration. provider_id is a stable text identifier
-- ("tmdb" / "tvdb" / "imdb" / "fanart" / "plugin:{id}"). All override columns
-- are nullable: null means "inherit global default / provider built-in".
create table if not exists metadata_provider_settings (
    provider_id text primary key,
    enabled boolean not null default true,
    api_base_url text,
    image_base_url text,
    proxy_mode text not null default 'inherit',
    proxy_url text,
    language text,
    country text,
    image_language text,
    image_prefer_original boolean,
    updated_at timestamptz not null default now(),
    check (length(trim(provider_id)) between 1 and 64),
    check (provider_id ~ '^[a-z0-9][a-z0-9:_-]*$'),
    check (proxy_mode in ('inherit', 'direct', 'custom')),
    -- custom proxy mode requires a proxy url; other modes must not carry one.
    check (
        (proxy_mode = 'custom' and proxy_url is not null and length(trim(proxy_url)) > 0)
        or (proxy_mode <> 'custom' and proxy_url is null)
    ),
    check (api_base_url is null or length(trim(api_base_url)) > 0),
    check (image_base_url is null or length(trim(image_base_url)) > 0),
    check (language is null or length(trim(language)) between 1 and 16),
    check (country is null or country ~ '^[A-Z]{2}$'),
    check (image_language is null or length(trim(image_language)) between 1 and 16)
);

-- Encrypted provider API keys / tokens. Mirrors plugin_config_secrets and
-- notification_target_secrets: XChaCha20-Poly1305 ciphertext, 24-byte nonce,
-- SHA-256 value hash. Kept in a dedicated table so the settings row can be read
-- freely without ever loading secret material, and the API never echoes keys.
create table if not exists metadata_provider_secrets (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    provider_id text not null,
    secret_key text not null,
    algorithm text not null,
    nonce bytea not null,
    ciphertext bytea not null,
    value_hash bytea not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (public_id),
    unique (provider_id, secret_key),
    check (length(trim(provider_id)) between 1 and 64),
    check (provider_id ~ '^[a-z0-9][a-z0-9:_-]*$'),
    check (length(trim(secret_key)) between 1 and 128),
    check (secret_key !~ '\s'),
    check (algorithm in ('xchacha20poly1305-sha256-key-v1')),
    check (length(nonce) = 24),
    check (length(ciphertext) > 0),
    check (length(value_hash) = 32)
);

create index if not exists idx_metadata_provider_secrets_provider
    on metadata_provider_secrets (provider_id, secret_key);
