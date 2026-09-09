alter table public.organizations
add column kind text not null default 'team'
check (kind in ('personal', 'team'));

create unique index organizations_single_personal_owner_idx
on public.organizations (owner_id)
where kind = 'personal';

create table public.user_profiles (
  id uuid primary key references auth.users(id) on delete cascade,
  display_name text not null check (char_length(display_name) between 1 and 64),
  avatar_path text,
  avatar_version bigint not null default 0 check (avatar_version >= 0),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  check (
    avatar_path is null
    or (
      split_part(avatar_path, '/', 1) = id::text
      and avatar_path ~ '^[0-9a-f-]{36}/[0-9a-f-]{36}\.webp$'
    )
  )
);

create or replace function private.can_view_account_profile(target_user_id text)
returns boolean
language sql
stable
security definer
set search_path = ''
as $$
  select (select auth.uid()) is not null
    and target_user_id is not null
    and (
      target_user_id = (select auth.uid())::text
      or exists (
        select 1
        from public.organization_members caller_membership
        join public.organization_members target_membership
          on target_membership.organization_id = caller_membership.organization_id
        where caller_membership.user_id = (select auth.uid())
          and target_membership.user_id::text = target_user_id
      )
    );
$$;

create or replace function private.reject_organization_kind_change()
returns trigger
language plpgsql
set search_path = ''
as $$
begin
  if new.kind <> old.kind then
    raise exception 'organization kind is immutable' using errcode = '22023';
  end if;
  return new;
end;
$$;

create trigger organizations_reject_kind_change
before update on public.organizations
for each row execute function private.reject_organization_kind_change();

create or replace function private.reject_personal_workspace_invite()
returns trigger
language plpgsql
security definer
set search_path = ''
as $$
begin
  if not exists (
    select 1
    from public.organizations organization
    where organization.id = new.organization_id
      and organization.kind = 'team'
  ) then
    raise exception 'personal workspace invitations are unavailable' using errcode = '42501';
  end if;
  return new;
end;
$$;

create trigger organization_invites_require_team
before insert or update on public.organization_invites
for each row execute function private.reject_personal_workspace_invite();

create or replace function public.ensure_personal_workspace()
returns public.organizations
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller_id uuid := (select auth.uid());
  workspace public.organizations;
begin
  if caller_id is null or not exists (
    select 1
    from auth.users account
    where account.id = caller_id
      and account.email_confirmed_at is not null
  ) then
    raise exception 'verified account required' using errcode = '42501';
  end if;

  insert into public.organizations (name, owner_id, kind)
  values ('personal', caller_id, 'personal')
  on conflict (owner_id) where kind = 'personal' do nothing;

  select * into workspace
  from public.organizations organization
  where organization.owner_id = caller_id
    and organization.kind = 'personal';

  if workspace.id is null then
    raise exception 'personal workspace unavailable' using errcode = '55000';
  end if;
  return workspace;
end;
$$;

create or replace function public.ensure_my_profile(target_display_name text)
returns public.user_profiles
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller_id uuid := (select auth.uid());
  normalized_name text := btrim(target_display_name);
  profile public.user_profiles;
begin
  if caller_id is null or normalized_name is null or char_length(normalized_name) not between 1 and 64 then
    raise exception 'invalid account profile' using errcode = '22023';
  end if;

  insert into public.user_profiles (id, display_name)
  values (caller_id, normalized_name)
  on conflict (id) do nothing;

  select * into profile
  from public.user_profiles account_profile
  where account_profile.id = caller_id;
  return profile;
end;
$$;

create or replace function public.update_my_display_name(target_display_name text)
returns public.user_profiles
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller_id uuid := (select auth.uid());
  normalized_name text := btrim(target_display_name);
  profile public.user_profiles;
begin
  if caller_id is null or normalized_name is null or char_length(normalized_name) not between 1 and 64 then
    raise exception 'invalid account profile' using errcode = '22023';
  end if;

  update public.user_profiles
  set display_name = normalized_name,
      updated_at = now()
  where id = caller_id
  returning * into profile;

  if profile.id is null then
    raise exception 'account profile unavailable' using errcode = '55000';
  end if;
  return profile;
end;
$$;

create or replace function public.set_my_avatar(
  target_avatar_path text,
  expected_avatar_version bigint
)
returns public.user_profiles
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller_id uuid := (select auth.uid());
  profile public.user_profiles;
begin
  if caller_id is null
    or expected_avatar_version < 0
    or (
      target_avatar_path is not null
      and (
        split_part(target_avatar_path, '/', 1) <> caller_id::text
        or target_avatar_path !~ '^[0-9a-f-]{36}/[0-9a-f-]{36}\.webp$'
      )
    )
  then
    raise exception 'invalid avatar update' using errcode = '22023';
  end if;

  update public.user_profiles
  set avatar_path = target_avatar_path,
      avatar_version = avatar_version + 1,
      updated_at = now()
  where id = caller_id
    and avatar_version = expected_avatar_version
  returning * into profile;

  if profile.id is null then
    raise exception 'avatar revision conflict' using errcode = '40001';
  end if;
  return profile;
end;
$$;

revoke all on function private.can_view_account_profile(text) from public, anon;
revoke all on function private.reject_organization_kind_change() from public, anon, authenticated;
revoke all on function private.reject_personal_workspace_invite() from public, anon, authenticated;
grant execute on function private.can_view_account_profile(text) to authenticated;

revoke all on function public.ensure_personal_workspace() from public, anon;
revoke all on function public.ensure_my_profile(text) from public, anon;
revoke all on function public.update_my_display_name(text) from public, anon;
revoke all on function public.set_my_avatar(text, bigint) from public, anon;
grant execute on function public.ensure_personal_workspace() to authenticated;
grant execute on function public.ensure_my_profile(text) to authenticated;
grant execute on function public.update_my_display_name(text) to authenticated;
grant execute on function public.set_my_avatar(text, bigint) to authenticated;

alter table public.user_profiles enable row level security;

create policy user_profiles_select on public.user_profiles for select to authenticated
using ((select private.can_view_account_profile(id::text)));
create policy user_profiles_insert on public.user_profiles for insert to authenticated
with check (id = (select auth.uid()));
create policy user_profiles_update on public.user_profiles for update to authenticated
using (id = (select auth.uid()))
with check (id = (select auth.uid()));

revoke all on table public.user_profiles from anon;
revoke all on table public.user_profiles from authenticated;
grant select on table public.user_profiles to authenticated;

insert into storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
values (
  'avatars', 'avatars', false, 2097152,
  array['image/jpeg', 'image/png', 'image/webp']::text[]
)
on conflict (id) do update
set public = excluded.public,
    file_size_limit = excluded.file_size_limit,
    allowed_mime_types = excluded.allowed_mime_types;

create policy avatars_select on storage.objects for select to authenticated
using (
  bucket_id = 'avatars'
  and (select private.can_view_account_profile((storage.foldername(name))[1]))
);

create policy avatars_insert on storage.objects for insert to authenticated
with check (
  bucket_id = 'avatars'
  and (storage.foldername(name))[1] = (select auth.uid())::text
);

create policy avatars_delete on storage.objects for delete to authenticated
using (
  bucket_id = 'avatars'
  and owner_id = (select auth.uid())::text
  and (storage.foldername(name))[1] = (select auth.uid())::text
);
