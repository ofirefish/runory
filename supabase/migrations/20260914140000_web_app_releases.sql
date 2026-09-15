-- Website release catalog for download pages. Desktop updater (latest.json) is unchanged.

create table public.app_releases (
  id uuid primary key default gen_random_uuid(),
  version text not null
    check (char_length(version) between 1 and 32)
    check (version ~ '^[A-Za-z0-9][A-Za-z0-9._+-]*$'),
  status text not null default 'draft'
    check (status in ('draft', 'published', 'archived')),
  is_latest boolean not null default false,
  notes_zh text
    check (notes_zh is null or char_length(notes_zh) <= 8000),
  notes_en text
    check (notes_en is null or char_length(notes_en) <= 8000),
  release_page_url text
    check (
      release_page_url is null
      or (
        char_length(release_page_url) between 12 and 2048
        and release_page_url ~ '^https://'
      )
    ),
  published_at timestamptz,
  created_by uuid not null references auth.users(id) on delete restrict,
  updated_by uuid not null references auth.users(id) on delete restrict,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  unique (version),
  check (not is_latest or status = 'published')
);

create unique index app_releases_one_latest_idx
on public.app_releases ((true))
where is_latest;

create index app_releases_status_published_at_idx
on public.app_releases (status, published_at desc nulls last, created_at desc);

create table public.app_release_assets (
  id uuid primary key default gen_random_uuid(),
  release_id uuid not null references public.app_releases(id) on delete cascade,
  platform text not null
    check (platform in ('windows', 'macos_apple', 'macos_intel', 'linux')),
  format text not null
    check (char_length(format) between 1 and 32),
  download_url text not null
    check (
      char_length(download_url) between 12 and 2048
      and download_url ~ '^https://'
    ),
  unique (release_id, platform)
);

create index app_release_assets_release_id_idx
on public.app_release_assets (release_id);

alter table public.app_releases enable row level security;
alter table public.app_release_assets enable row level security;

create policy app_releases_public_select
on public.app_releases
for select
to anon, authenticated
using (status = 'published');

create policy app_release_assets_public_select
on public.app_release_assets
for select
to anon, authenticated
using (
  exists (
    select 1
    from public.app_releases release
    where release.id = app_release_assets.release_id
      and release.status = 'published'
  )
);

revoke all on table public.app_releases, public.app_release_assets from public, anon, authenticated, service_role;

grant select on table public.app_releases, public.app_release_assets to anon, authenticated;
grant select, insert, update, delete on table public.app_releases, public.app_release_assets to service_role;
