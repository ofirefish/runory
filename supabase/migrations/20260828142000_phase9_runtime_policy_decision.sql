create or replace function public.evaluate_access_policy(
  target_organization_id uuid,
  target_action text,
  target_resource_type text,
  target_resource_id uuid
)
returns boolean
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller_id uuid := (select auth.uid());
  allowed boolean;
begin
  if caller_id is null
    or target_action not in ('connect', 'read-files', 'write-files', 'operate', 'deploy', 'ai-execute')
    or target_resource_type <> 'server-profile'
    or target_resource_id is null
    or not exists (
      select 1 from public.organization_members member
      where member.organization_id = target_organization_id and member.user_id = caller_id
    )
  then
    return false;
  end if;

  select
    not exists (
      select 1 from public.access_policies policy
      where policy.organization_id = target_organization_id
        and policy.action = target_action
        and policy.effect = 'deny'
        and (
          policy.resource_selector = '{}'::jsonb
          or (
            jsonb_typeof(policy.resource_selector -> 'profileIds') = 'array'
            and (policy.resource_selector -> 'profileIds') ? target_resource_id::text
          )
        )
    )
    and (
      not exists (
        select 1 from public.access_policies policy
        where policy.organization_id = target_organization_id
          and policy.action = target_action and policy.effect = 'allow'
      )
      or exists (
        select 1 from public.access_policies policy
        where policy.organization_id = target_organization_id
          and policy.action = target_action
          and policy.effect = 'allow'
          and (
            policy.resource_selector = '{}'::jsonb
            or (
              jsonb_typeof(policy.resource_selector -> 'profileIds') = 'array'
              and (policy.resource_selector -> 'profileIds') ? target_resource_id::text
            )
          )
      )
    )
  into allowed;

  insert into public.audit_records (
    organization_id, actor_id, action, resource_type, resource_id, result, error_code
  ) values (
    target_organization_id, caller_id, 'policy.evaluate', target_resource_type,
    target_resource_id::text, case when allowed then 'succeeded' else 'denied' end,
    case when allowed then null else 'CLOUD_POLICY_DENIED' end
  );

  return allowed;
end;
$$;

revoke all on function public.evaluate_access_policy(uuid, text, text, uuid) from public, anon;
grant execute on function public.evaluate_access_policy(uuid, text, text, uuid) to authenticated;

comment on function public.evaluate_access_policy(uuid, text, text, uuid) is
  'Fail-closed membership check with deny-overrides evaluation for Rust typed operations.';
