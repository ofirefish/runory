-- Plugin Marketplace catalog for desktop Discover (compatible with Core MarketplaceClient).

create table public.marketplace_plugins (
  id text primary key
    check (char_length(id) between 3 and 128)
    check (id ~ '^[a-z0-9][a-z0-9._-]*$'),
  publisher_id text not null
    check (char_length(publisher_id) between 1 and 64),
  name text not null
    check (char_length(name) between 1 and 128),
  description text
    check (description is null or char_length(description) <= 4000),
  category text
    check (category is null or char_length(category) <= 64),
  latest_version text not null
    check (char_length(latest_version) between 1 and 64),
  api_version text not null default '1'
    check (char_length(api_version) between 1 and 16),
  verified boolean not null default false,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create index marketplace_plugins_publisher_idx
  on public.marketplace_plugins (publisher_id);

create index marketplace_plugins_name_idx
  on public.marketplace_plugins (name);

create table public.marketplace_plugin_versions (
  plugin_id text not null references public.marketplace_plugins(id) on delete cascade,
  version text not null
    check (char_length(version) between 1 and 64),
  api_version text not null default '1'
    check (char_length(api_version) between 1 and 16),
  min_runory_version text
    check (min_runory_version is null or char_length(min_runory_version) <= 32),
  package_sha256 text not null
    check (package_sha256 ~ '^[a-f0-9]{64}$'),
  storage_path text not null
    check (char_length(storage_path) between 8 and 512),
  status text not null default 'published'
    check (status in ('draft', 'published', 'deprecated', 'revoked')),
  changelog text
    check (changelog is null or char_length(changelog) <= 8000),
  permissions jsonb not null default '[]'::jsonb
    check (jsonb_typeof(permissions) = 'array'),
  published_at timestamptz not null default now(),
  primary key (plugin_id, version)
);

create index marketplace_plugin_versions_status_idx
  on public.marketplace_plugin_versions (plugin_id, status);

create table public.marketplace_publisher_keys (
  publisher_id text not null
    check (char_length(publisher_id) between 1 and 64),
  key_id text not null
    check (char_length(key_id) between 1 and 128),
  algorithm text not null default 'ed25519'
    check (algorithm = 'ed25519'),
  public_key text not null
    check (char_length(public_key) between 40 and 128),
  status text not null default 'active'
    check (status in ('active', 'revoked')),
  created_at timestamptz not null default now(),
  primary key (publisher_id, key_id)
);

create table public.marketplace_revocations (
  kind text not null
    check (kind in ('plugin', 'plugin_version', 'publisher_key')),
  target_id text not null
    check (char_length(target_id) between 1 and 128),
  version text not null default ''
    check (char_length(version) <= 64),
  reason text
    check (reason is null or char_length(reason) <= 1000),
  created_at timestamptz not null default now(),
  primary key (kind, target_id, version)
);

alter table public.marketplace_plugins enable row level security;
alter table public.marketplace_plugin_versions enable row level security;
alter table public.marketplace_publisher_keys enable row level security;
alter table public.marketplace_revocations enable row level security;

create policy marketplace_plugins_public_select
on public.marketplace_plugins
for select
to anon, authenticated
using (true);

create policy marketplace_plugin_versions_public_select
on public.marketplace_plugin_versions
for select
to anon, authenticated
using (status = 'published');

create policy marketplace_publisher_keys_public_select
on public.marketplace_publisher_keys
for select
to anon, authenticated
using (status = 'active');

create policy marketplace_revocations_public_select
on public.marketplace_revocations
for select
to anon, authenticated
using (true);

revoke all on table
  public.marketplace_plugins,
  public.marketplace_plugin_versions,
  public.marketplace_publisher_keys,
  public.marketplace_revocations
from public, anon, authenticated, service_role;

grant select on table
  public.marketplace_plugins,
  public.marketplace_plugin_versions,
  public.marketplace_publisher_keys,
  public.marketplace_revocations
to anon, authenticated;

grant select, insert, update, delete on table
  public.marketplace_plugins,
  public.marketplace_plugin_versions,
  public.marketplace_publisher_keys,
  public.marketplace_revocations
to service_role;

-- Private package bucket; downloads go through signed URLs / API proxy.
insert into storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
values (
  'plugin-packages',
  'plugin-packages',
  false,
  52428800,
  array[
    'application/octet-stream',
    'application/zip',
    'application/x-zip-compressed'
  ]::text[]
)
on conflict (id) do update
set
  public = excluded.public,
  file_size_limit = excluded.file_size_limit,
  allowed_mime_types = excluded.allowed_mime_types;

-- No anon/authenticated storage policies: only service_role uploads/downloads.

-- Official Runory publisher key (matches plugin-sdk/keys/runory-2026-main.pub.json).
insert into public.marketplace_publisher_keys (publisher_id, key_id, algorithm, public_key, status)
values (
  'runory',
  'runory-2026-main',
  'ed25519',
  'YfFuwISbG9zyHwlChH6PPyQ6rueVJp7ofqFC1VGndiI=',
  'active'
)
on conflict (publisher_id, key_id) do update
set
  public_key = excluded.public_key,
  algorithm = excluded.algorithm,
  status = excluded.status;
