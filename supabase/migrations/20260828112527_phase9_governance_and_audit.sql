create or replace function public.update_organization_member_role(
  target_organization_id uuid,
  target_user_id uuid,
  target_role text
)
returns public.organization_members
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller_id uuid := (select auth.uid());
  caller_role text := coalesce((select private.organization_role(target_organization_id)), '');
  current_member public.organization_members;
  updated_member public.organization_members;
begin
  select * into current_member
  from public.organization_members
  where organization_id = target_organization_id and user_id = target_user_id
  for update;

  if caller_id is null
    or current_member.user_id is null
    or current_member.role = 'owner'
    or target_role not in ('admin', 'operator', 'viewer')
    or not (
      caller_role = 'owner'
      or (caller_role = 'admin' and current_member.role in ('operator', 'viewer') and target_role in ('operator', 'viewer'))
    )
  then
    raise exception 'member update is unavailable' using errcode = '42501';
  end if;

  update public.organization_members
  set role = target_role
  where organization_id = target_organization_id and user_id = target_user_id
  returning * into updated_member;

  insert into public.audit_records (
    organization_id, actor_id, action, resource_type, resource_id, result
  ) values (
    target_organization_id, caller_id, 'member.role.update', 'organization-member',
    target_user_id::text, 'succeeded'
  );
  return updated_member;
end;
$$;

create or replace function public.remove_organization_member(
  target_organization_id uuid,
  target_user_id uuid
)
returns void
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller_id uuid := (select auth.uid());
  caller_role text := coalesce((select private.organization_role(target_organization_id)), '');
  current_member public.organization_members;
begin
  select * into current_member
  from public.organization_members
  where organization_id = target_organization_id and user_id = target_user_id
  for update;

  if caller_id is null
    or current_member.user_id is null
    or current_member.role = 'owner'
    or not (
      caller_role = 'owner'
      or (caller_role = 'admin' and current_member.role in ('operator', 'viewer'))
    )
  then
    raise exception 'member removal is unavailable' using errcode = '42501';
  end if;

  delete from public.organization_members
  where organization_id = target_organization_id and user_id = target_user_id;

  insert into public.audit_records (
    organization_id, actor_id, action, resource_type, resource_id, result
  ) values (
    target_organization_id, caller_id, 'member.remove', 'organization-member',
    target_user_id::text, 'succeeded'
  );
end;
$$;

create or replace function public.create_access_policy(
  target_organization_id uuid,
  target_name text,
  target_effect text,
  target_action text,
  target_resource_selector jsonb
)
returns public.access_policies
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller_id uuid := (select auth.uid());
  policy public.access_policies;
begin
  if caller_id is null
    or coalesce((select private.organization_role(target_organization_id)), '') not in ('owner', 'admin')
    or char_length(btrim(target_name)) not between 1 and 100
    or target_effect not in ('allow', 'deny')
    or target_action not in ('connect', 'read-files', 'write-files', 'operate', 'deploy', 'ai-execute')
    or jsonb_typeof(target_resource_selector) <> 'object'
    or octet_length(target_resource_selector::text) > 16384
  then
    raise exception 'invalid access policy' using errcode = '22023';
  end if;

  insert into public.access_policies (
    organization_id, name, effect, action, resource_selector, created_by
  ) values (
    target_organization_id, btrim(target_name), target_effect, target_action,
    target_resource_selector, caller_id
  ) returning * into policy;

  insert into public.audit_records (
    organization_id, actor_id, action, resource_type, resource_id, result
  ) values (
    target_organization_id, caller_id, 'access-policy.create', 'access-policy',
    policy.id::text, 'succeeded'
  );
  return policy;
end;
$$;

create or replace function public.delete_access_policy(target_policy_id uuid)
returns void
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller_id uuid := (select auth.uid());
  policy public.access_policies;
begin
  select * into policy
  from public.access_policies
  where id = target_policy_id
  for update;

  if caller_id is null
    or policy.id is null
    or coalesce((select private.organization_role(policy.organization_id)), '') not in ('owner', 'admin')
  then
    raise exception 'access policy is unavailable' using errcode = '42501';
  end if;

  delete from public.access_policies where id = policy.id;
  insert into public.audit_records (
    organization_id, actor_id, action, resource_type, resource_id, result
  ) values (
    policy.organization_id, caller_id, 'access-policy.delete', 'access-policy',
    policy.id::text, 'succeeded'
  );
end;
$$;

create or replace function public.list_audit_records(
  target_organization_id uuid,
  cursor_occurred_at timestamptz,
  cursor_id bigint,
  target_page_size integer
)
returns table (
  id bigint,
  organization_id uuid,
  actor_id uuid,
  action text,
  resource_type text,
  resource_id text,
  result text,
  error_code text,
  occurred_at timestamptz
)
language sql
stable
security invoker
set search_path = ''
as $$
  select record.id, record.organization_id, record.actor_id, record.action,
         record.resource_type, record.resource_id, record.result,
         record.error_code, record.occurred_at
  from public.audit_records record
  where record.organization_id = target_organization_id
    and (
      cursor_occurred_at is null
      or cursor_id is null
      or (record.occurred_at, record.id) < (cursor_occurred_at, cursor_id)
    )
  order by record.occurred_at desc, record.id desc
  limit least(greatest(coalesce(target_page_size, 50), 1), 100);
$$;

create or replace function private.prune_audit_records(retention_days integer default 180)
returns bigint
language plpgsql
security definer
set search_path = ''
as $$
declare
  removed bigint;
begin
  if retention_days not between 30 and 3650 then
    raise exception 'invalid audit retention' using errcode = '22023';
  end if;
  delete from public.audit_records
  where occurred_at < now() - make_interval(days => retention_days);
  get diagnostics removed = row_count;
  return removed;
end;
$$;

revoke all on function public.update_organization_member_role(uuid, uuid, text) from public, anon;
revoke all on function public.remove_organization_member(uuid, uuid) from public, anon;
revoke all on function public.create_access_policy(uuid, text, text, text, jsonb) from public, anon;
revoke all on function public.delete_access_policy(uuid) from public, anon;
revoke all on function public.list_audit_records(uuid, timestamptz, bigint, integer) from public, anon;
revoke all on function private.prune_audit_records(integer) from public, anon, authenticated;

grant execute on function public.update_organization_member_role(uuid, uuid, text) to authenticated;
grant execute on function public.remove_organization_member(uuid, uuid) to authenticated;
grant execute on function public.create_access_policy(uuid, text, text, text, jsonb) to authenticated;
grant execute on function public.delete_access_policy(uuid) to authenticated;
grant execute on function public.list_audit_records(uuid, timestamptz, bigint, integer) to authenticated;

revoke insert, update, delete on table public.organization_members from authenticated;
revoke insert, update, delete on table public.access_policies from authenticated;

comment on function private.prune_audit_records(integer) is
  'Privileged retention function intended for a Supabase Cron job configured by the project owner.';
