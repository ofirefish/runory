begin;
create extension if not exists pgtap with schema extensions;
set local search_path = public, extensions;
select plan(63);

select ok(
  (select bool_and(class.relrowsecurity)
   from pg_class class
   join pg_namespace namespace on namespace.oid = class.relnamespace
   where namespace.nspname = 'public'
     and class.relname = any (array[
       'organizations', 'organization_members', 'organization_invites',
       'sync_objects', 'access_policies', 'audit_records'
     ])),
  'all cloud business tables have RLS enabled'
);

select ok(not has_table_privilege('anon', 'public.organizations', 'SELECT'), 'anon cannot read organizations');
select ok(not has_table_privilege('anon', 'public.sync_objects', 'SELECT'), 'anon cannot read sync objects');
select ok(not has_table_privilege('authenticated', 'public.sync_objects', 'INSERT'), 'authenticated cannot bypass sync insert RPC');
select ok(not has_table_privilege('authenticated', 'public.sync_objects', 'UPDATE'), 'authenticated cannot bypass sync update RPC');
select ok(has_function_privilege('authenticated', 'public.write_sync_object(uuid,text,uuid,jsonb,bigint)', 'EXECUTE'), 'authenticated can execute revision writer');
select ok(not has_function_privilege('anon', 'public.write_sync_object(uuid,text,uuid,jsonb,bigint)', 'EXECUTE'), 'anon cannot execute revision writer');
select ok(has_function_privilege('authenticated', 'public.accept_organization_invite(uuid)', 'EXECUTE'), 'authenticated can accept a matching invite');
select ok(not has_function_privilege('anon', 'public.accept_organization_invite(uuid)', 'EXECUTE'), 'anon cannot accept invites');
select ok(not has_table_privilege('authenticated', 'public.organization_invites', 'INSERT'), 'authenticated cannot bypass invitation RPC');
select ok(not has_table_privilege('authenticated', 'public.organization_invites', 'DELETE'), 'authenticated cannot bypass invitation revoke RPC');
select ok(has_function_privilege('authenticated', 'public.create_organization_invite(uuid,text,text)', 'EXECUTE'), 'authenticated can call invitation creator');
select ok(not has_function_privilege('anon', 'public.create_organization_invite(uuid,text,text)', 'EXECUTE'), 'anon cannot create invitations');
select ok(has_function_privilege('authenticated', 'public.list_my_organization_invites()', 'EXECUTE'), 'authenticated can list only matching invitations');
select ok(not has_function_privilege('anon', 'public.list_my_organization_invites()', 'EXECUTE'), 'anon cannot list invitations');
select ok(not has_function_privilege('anon', 'public.list_organization_members(uuid)', 'EXECUTE'), 'anon cannot list organization members');
select ok(has_function_privilege('authenticated', 'public.revoke_organization_invite(uuid)', 'EXECUTE'), 'authenticated can call audited invitation revoke');
select ok(not has_function_privilege('anon', 'public.revoke_organization_invite(uuid)', 'EXECUTE'), 'anon cannot revoke invitations');
select ok(not has_table_privilege('authenticated', 'public.organization_members', 'UPDATE'), 'authenticated cannot bypass member role RPC');
select ok(not has_table_privilege('authenticated', 'public.organization_members', 'DELETE'), 'authenticated cannot bypass member removal RPC');
select ok(has_function_privilege('authenticated', 'public.update_organization_member_role(uuid,uuid,text)', 'EXECUTE'), 'authenticated can call constrained member role RPC');
select ok(not has_function_privilege('anon', 'public.update_organization_member_role(uuid,uuid,text)', 'EXECUTE'), 'anon cannot update member roles');
select ok(has_function_privilege('authenticated', 'public.remove_organization_member(uuid,uuid)', 'EXECUTE'), 'authenticated can call constrained member removal RPC');
select ok(not has_function_privilege('anon', 'public.remove_organization_member(uuid,uuid)', 'EXECUTE'), 'anon cannot remove members');
select ok(not has_table_privilege('authenticated', 'public.access_policies', 'INSERT'), 'authenticated cannot bypass policy creation RPC');
select ok(not has_table_privilege('authenticated', 'public.access_policies', 'UPDATE'), 'authenticated cannot update policies directly');
select ok(not has_table_privilege('authenticated', 'public.access_policies', 'DELETE'), 'authenticated cannot bypass policy deletion RPC');
select ok(has_function_privilege('authenticated', 'public.create_access_policy(uuid,text,text,text,jsonb)', 'EXECUTE'), 'authenticated can call audited policy creation');
select ok(not has_function_privilege('anon', 'public.create_access_policy(uuid,text,text,text,jsonb)', 'EXECUTE'), 'anon cannot create policies');
select ok(has_function_privilege('authenticated', 'public.delete_access_policy(uuid)', 'EXECUTE'), 'authenticated can call audited policy deletion');
select ok(has_function_privilege('authenticated', 'public.list_audit_records(uuid,timestamptz,bigint,integer)', 'EXECUTE'), 'authenticated can call RLS-bound audit pagination');
select ok(not has_function_privilege('anon', 'public.list_audit_records(uuid,timestamptz,bigint,integer)', 'EXECUTE'), 'anon cannot list audit records');
select ok(not has_function_privilege('authenticated', 'private.prune_audit_records(integer)', 'EXECUTE'), 'client cannot execute privileged audit retention');
select ok(has_function_privilege('authenticated', 'public.evaluate_access_policy(uuid,text,text,uuid)', 'EXECUTE'), 'authenticated can evaluate typed operation policy');
select ok(not has_function_privilege('anon', 'public.evaluate_access_policy(uuid,text,text,uuid)', 'EXECUTE'), 'anon cannot evaluate typed operation policy');

select has_index('public', 'organizations', 'organizations_owner_id_idx', 'organization owner foreign key is indexed');
select has_index('public', 'organization_invites', 'organization_invites_invited_by_idx', 'invitation actor foreign key is indexed');
select has_index('public', 'sync_objects', 'sync_objects_updated_by_idx', 'sync actor foreign key is indexed');
select has_index('public', 'access_policies', 'access_policies_created_by_idx', 'policy actor foreign key is indexed');

select ok(
  not exists (
    select 1
    from pg_proc procedure
    join pg_namespace namespace on namespace.oid = procedure.pronamespace
    where namespace.nspname in ('public', 'private')
      and procedure.prosecdef
      and not (procedure.proconfig @> array['search_path=""'])
  ),
  'all Runory security definer functions pin an empty search_path'
);

select ok(
  not exists (
    select 1
    from pg_proc procedure
    join pg_namespace namespace on namespace.oid = procedure.pronamespace
    cross join lateral aclexplode(coalesce(procedure.proacl, acldefault('f', procedure.proowner))) privilege
    where namespace.nspname in ('public', 'private')
      and procedure.prosecdef
      and privilege.grantee = 0
      and privilege.privilege_type = 'EXECUTE'
  ),
  'no Runory security definer function is executable by PUBLIC'
);

insert into auth.users (id, email, email_confirmed_at) values
  ('00000000-0000-0000-0000-000000000001', 'owner-a@example.test', now()),
  ('00000000-0000-0000-0000-000000000002', 'owner-b@example.test', now()),
  ('00000000-0000-0000-0000-000000000003', 'viewer@example.test', now());

set local role authenticated;
set local "request.jwt.claim.sub" = '00000000-0000-0000-0000-000000000001';
insert into public.organizations (id, name, owner_id) values (
  '10000000-0000-0000-0000-000000000001', 'Organization A',
  '00000000-0000-0000-0000-000000000001'
);
reset role;

set local role authenticated;
set local "request.jwt.claim.sub" = '00000000-0000-0000-0000-000000000002';
insert into public.organizations (id, name, owner_id) values (
  '10000000-0000-0000-0000-000000000002', 'Organization B',
  '00000000-0000-0000-0000-000000000002'
);
reset role;

insert into public.organization_members (organization_id, user_id, role) values (
  '10000000-0000-0000-0000-000000000001',
  '00000000-0000-0000-0000-000000000003', 'viewer'
);

set local role authenticated;
set local "request.jwt.claim.sub" = '00000000-0000-0000-0000-000000000001';
select results_eq(
  'select count(*) from public.organizations', array[1::bigint],
  'owner sees only their organization through RLS'
);
select results_eq(
  $$select count(*) from public.organizations where id = '10000000-0000-0000-0000-000000000002'$$,
  array[0::bigint], 'owner cannot read another organization'
);
select throws_ok(
  $$insert into public.organization_members (organization_id, user_id, role) values ('10000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000002', 'admin')$$,
  '42501', 'permission denied for table organization_members',
  'authenticated clients cannot bypass the member RPC'
);
select throws_ok(
  $$insert into public.access_policies (organization_id, name, effect, action, created_by) values ('10000000-0000-0000-0000-000000000001', 'Bypass', 'allow', 'connect', '00000000-0000-0000-0000-000000000001')$$,
  '42501', 'permission denied for table access_policies',
  'authenticated clients cannot bypass the policy RPC'
);
select is(
  public.evaluate_access_policy(
    '10000000-0000-0000-0000-000000000001', 'operate', 'server-profile',
    '20000000-0000-0000-0000-000000000001'
  ), true, 'member is allowed when no allow or deny policy exists'
);
select lives_ok(
  $$select public.create_access_policy('10000000-0000-0000-0000-000000000001', 'Block connect', 'deny', 'connect', '{}'::jsonb)$$,
  'owner can create a deny policy through the audited RPC'
);
select is(
  public.evaluate_access_policy(
    '10000000-0000-0000-0000-000000000001', 'connect', 'server-profile',
    '20000000-0000-0000-0000-000000000001'
  ), false, 'deny policy overrides the default allow decision'
);
select ok(
  (select count(*) from public.audit_records where organization_id = '10000000-0000-0000-0000-000000000001') >= 3,
  'policy mutations and decisions create organization-scoped audit records'
);

set local "request.jwt.claim.sub" = '00000000-0000-0000-0000-000000000002';
select is(
  public.evaluate_access_policy(
    '10000000-0000-0000-0000-000000000001', 'operate', 'server-profile',
    '20000000-0000-0000-0000-000000000001'
  ), false, 'non-member policy evaluation fails closed'
);

set local "request.jwt.claim.sub" = '00000000-0000-0000-0000-000000000003';
select results_eq(
  'select count(*) from public.organizations', array[1::bigint],
  'viewer sees only organizations where they are a member'
);
reset role;

select has_extension('pg_cron', 'Supabase Cron extension is enabled');
select has_index(
  'public', 'audit_records', 'audit_records_retention_idx',
  'audit retention scan has an occurred-at index'
);
select is(
  (
    select count(*)
    from cron.job
    where jobname = 'runory-audit-retention-daily'
      and schedule = '17 3 * * *'
      and command = 'select private.prune_audit_records(180);'
      and active
  ), 1::bigint, 'daily bounded audit retention job is active'
);
select ok(not has_schema_privilege('anon', 'cron', 'USAGE'), 'anon cannot access the Cron schema');
select ok(not has_schema_privilege('authenticated', 'cron', 'USAGE'), 'authenticated cannot access the Cron schema');
set local role anon;
select throws_ok(
  $$select count(*) from cron.job$$, '42501', 'permission denied for schema cron',
  'anon cannot inspect Cron jobs through extension table ACL'
);
reset role;
set local role authenticated;
select throws_ok(
  $$select cron.schedule('client-bypass', '* * * * *', 'select 1')$$,
  '42501', 'permission denied for schema cron',
  'authenticated cannot schedule Cron jobs through extension function ACL'
);
reset role;

insert into public.audit_records (
  organization_id, actor_id, action, resource_type, resource_id, result, occurred_at
)
select
  '10000000-0000-0000-0000-000000000001',
  '00000000-0000-0000-0000-000000000001',
  'retention.test', 'audit-record', value::text, 'succeeded', now() - interval '181 days'
from generate_series(1, 10001) value;
insert into public.audit_records (
  organization_id, actor_id, action, resource_type, resource_id, result, occurred_at
) values (
  '10000000-0000-0000-0000-000000000001',
  '00000000-0000-0000-0000-000000000001',
  'retention.test', 'audit-record', 'recent', 'succeeded', now()
);
select is(
  private.prune_audit_records(180), 10000::bigint,
  'one retention run deletes at most ten thousand records'
);
select is(
  (
    select count(*) from public.audit_records
    where action = 'retention.test' and occurred_at < now() - interval '180 days'
  ), 1::bigint, 'expired audit backlog continues in the next bounded run'
);
select is(
  (
    select count(*) from public.audit_records
    where action = 'retention.test' and resource_id = 'recent'
  ), 1::bigint, 'retention never deletes records inside the retention window'
);
select throws_ok(
  $$select private.prune_audit_records(29)$$,
  '22023', 'invalid audit retention', 'retention below thirty days is rejected'
);
select throws_ok(
  $$select private.prune_audit_records(3651)$$,
  '22023', 'invalid audit retention', 'retention above ten years is rejected'
);

select * from finish();
rollback;
