create schema if not exists private;

create table public.organizations (
  id uuid primary key default gen_random_uuid(),
  name text not null check (char_length(name) between 1 and 100),
  owner_id uuid not null references auth.users(id) on delete restrict,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create table public.organization_members (
  organization_id uuid not null references public.organizations(id) on delete cascade,
  user_id uuid not null references auth.users(id) on delete cascade,
  role text not null check (role in ('owner', 'admin', 'operator', 'viewer')),
  created_at timestamptz not null default now(),
  primary key (organization_id, user_id)
);

create index organization_members_user_id_idx on public.organization_members (user_id);

create table public.organization_invites (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  email text not null check (char_length(email) between 3 and 320),
  role text not null check (role in ('admin', 'operator', 'viewer')),
  invited_by uuid not null references auth.users(id) on delete cascade,
  expires_at timestamptz not null,
  accepted_at timestamptz,
  created_at timestamptz not null default now(),
  unique (organization_id, email)
);

create index organization_invites_organization_id_idx on public.organization_invites (organization_id);
create index organization_invites_email_pending_idx on public.organization_invites (lower(email)) where accepted_at is null;

create table public.sync_objects (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  kind text not null check (kind in ('profile', 'group', 'inventory', 'known-host')),
  logical_id uuid not null,
  encrypted_payload jsonb not null,
  revision bigint not null default 1 check (revision > 0),
  updated_by uuid not null references auth.users(id) on delete restrict,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  unique (organization_id, kind, logical_id),
  check (jsonb_typeof(encrypted_payload) = 'object')
);

create index sync_objects_organization_updated_idx on public.sync_objects (organization_id, updated_at desc);

create table public.access_policies (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  name text not null check (char_length(name) between 1 and 100),
  effect text not null check (effect in ('allow', 'deny')),
  action text not null check (action in ('connect', 'read-files', 'write-files', 'operate', 'deploy', 'ai-execute')),
  resource_selector jsonb not null default '{}'::jsonb,
  created_by uuid not null references auth.users(id) on delete restrict,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  check (jsonb_typeof(resource_selector) = 'object')
);

create index access_policies_organization_id_idx on public.access_policies (organization_id);

create table public.audit_records (
  id bigint generated always as identity primary key,
  organization_id uuid not null references public.organizations(id) on delete cascade,
  actor_id uuid not null references auth.users(id) on delete restrict,
  action text not null check (char_length(action) between 1 and 100),
  resource_type text not null check (char_length(resource_type) between 1 and 64),
  resource_id text,
  result text not null check (result in ('started', 'succeeded', 'failed', 'denied')),
  error_code text,
  occurred_at timestamptz not null default now()
);

create index audit_records_organization_time_idx on public.audit_records (organization_id, occurred_at desc, id desc);
create index audit_records_actor_id_idx on public.audit_records (actor_id);

create or replace function private.is_organization_member(target_organization_id uuid)
returns boolean
language sql
stable
security definer
set search_path = ''
as $$
  select (select auth.uid()) is not null and exists (
    select 1
    from public.organization_members member
    where member.organization_id = target_organization_id
      and member.user_id = (select auth.uid())
  );
$$;

create or replace function private.organization_role(target_organization_id uuid)
returns text
language sql
stable
security definer
set search_path = ''
as $$
  select member.role
  from public.organization_members member
  where member.organization_id = target_organization_id
    and member.user_id = (select auth.uid())
  limit 1;
$$;

create or replace function private.add_organization_owner()
returns trigger
language plpgsql
security definer
set search_path = ''
as $$
begin
  if new.owner_id <> (select auth.uid()) then
    raise exception 'organization owner must be current user';
  end if;
  insert into public.organization_members (organization_id, user_id, role)
  values (new.id, new.owner_id, 'owner');
  return new;
end;
$$;

create trigger organizations_add_owner
after insert on public.organizations
for each row execute function private.add_organization_owner();

revoke all on schema private from public, anon, authenticated;
grant usage on schema private to authenticated;
revoke all on all functions in schema private from public, anon, authenticated;
grant execute on function private.is_organization_member(uuid) to authenticated;
grant execute on function private.organization_role(uuid) to authenticated;

alter table public.organizations enable row level security;
alter table public.organization_members enable row level security;
alter table public.organization_invites enable row level security;
alter table public.sync_objects enable row level security;
alter table public.access_policies enable row level security;
alter table public.audit_records enable row level security;

create policy organizations_select on public.organizations for select to authenticated
using ((select private.is_organization_member(id)));
create policy organizations_insert on public.organizations for insert to authenticated
with check (owner_id = (select auth.uid()));
create policy organizations_update on public.organizations for update to authenticated
using (owner_id = (select auth.uid())) with check (owner_id = (select auth.uid()));
create policy organizations_delete on public.organizations for delete to authenticated
using (owner_id = (select auth.uid()));

create policy members_select on public.organization_members for select to authenticated
using ((select private.is_organization_member(organization_id)));
create policy members_insert on public.organization_members for insert to authenticated
with check (
  (select private.organization_role(organization_id)) in ('owner', 'admin')
  and role <> 'owner'
);
create policy members_update on public.organization_members for update to authenticated
using (
  (select private.organization_role(organization_id)) in ('owner', 'admin')
  and role <> 'owner'
)
with check (
  (select private.organization_role(organization_id)) in ('owner', 'admin')
  and role <> 'owner'
);
create policy members_delete on public.organization_members for delete to authenticated
using ((select private.organization_role(organization_id)) in ('owner', 'admin') and role <> 'owner');

create policy invites_select on public.organization_invites for select to authenticated
using ((select private.organization_role(organization_id)) in ('owner', 'admin'));
create policy invites_insert on public.organization_invites for insert to authenticated
with check (invited_by = (select auth.uid()) and (select private.organization_role(organization_id)) in ('owner', 'admin'));
create policy invites_delete on public.organization_invites for delete to authenticated
using ((select private.organization_role(organization_id)) in ('owner', 'admin'));

create policy sync_objects_select on public.sync_objects for select to authenticated
using ((select private.is_organization_member(organization_id)));
create policy sync_objects_insert on public.sync_objects for insert to authenticated
with check (updated_by = (select auth.uid()) and (select private.organization_role(organization_id)) in ('owner', 'admin', 'operator'));
create policy sync_objects_update on public.sync_objects for update to authenticated
using ((select private.organization_role(organization_id)) in ('owner', 'admin', 'operator'))
with check (updated_by = (select auth.uid()) and (select private.organization_role(organization_id)) in ('owner', 'admin', 'operator'));
create policy sync_objects_delete on public.sync_objects for delete to authenticated
using ((select private.organization_role(organization_id)) in ('owner', 'admin'));

create policy access_policies_select on public.access_policies for select to authenticated
using ((select private.is_organization_member(organization_id)));
create policy access_policies_insert on public.access_policies for insert to authenticated
with check (created_by = (select auth.uid()) and (select private.organization_role(organization_id)) in ('owner', 'admin'));
create policy access_policies_update on public.access_policies for update to authenticated
using ((select private.organization_role(organization_id)) in ('owner', 'admin'))
with check ((select private.organization_role(organization_id)) in ('owner', 'admin'));
create policy access_policies_delete on public.access_policies for delete to authenticated
using ((select private.organization_role(organization_id)) in ('owner', 'admin'));

create policy audit_records_select on public.audit_records for select to authenticated
using ((select private.organization_role(organization_id)) in ('owner', 'admin'));
create policy audit_records_insert on public.audit_records for insert to authenticated
with check (actor_id = (select auth.uid()) and (select private.is_organization_member(organization_id)));

revoke all on table public.organizations, public.organization_members, public.organization_invites,
  public.sync_objects, public.access_policies, public.audit_records from anon;
grant select, insert, update, delete on table public.organizations, public.organization_members,
  public.organization_invites, public.sync_objects, public.access_policies to authenticated;
grant select, insert on table public.audit_records to authenticated;
grant usage, select on sequence public.audit_records_id_seq to authenticated;
