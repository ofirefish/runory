begin;
create extension if not exists pgtap with schema extensions;
set local search_path = public, extensions;
select plan(18);

select has_table('public', 'platform_admins', 'platform admin registry exists');
select has_table('public', 'platform_admin_audit', 'platform admin audit exists');
select ok((select relrowsecurity from pg_class where oid = 'public.platform_admins'::regclass), 'admin registry has RLS enabled');
select ok((select relrowsecurity from pg_class where oid = 'public.platform_admin_audit'::regclass), 'admin audit has RLS enabled');
select ok(not has_table_privilege('anon', 'public.platform_admins', 'SELECT'), 'anon cannot inspect administrators');
select ok(not has_table_privilege('authenticated', 'public.platform_admins', 'SELECT'), 'authenticated users cannot inspect administrators');
select ok(not has_table_privilege('authenticated', 'public.platform_admin_audit', 'SELECT'), 'authenticated users cannot inspect audit records');
select ok(not has_table_privilege('authenticated', 'public.platform_admin_audit', 'INSERT'), 'authenticated users cannot write audit records');
select ok(has_table_privilege('service_role', 'public.platform_admins', 'SELECT'), 'service role can authorize an administrator');
select ok(has_table_privilege('service_role', 'public.platform_admin_audit', 'SELECT'), 'service role can read audit records');
select ok(has_table_privilege('service_role', 'public.platform_admin_audit', 'INSERT'), 'service role can append audit records');
select ok(not has_table_privilege('service_role', 'public.platform_admins', 'INSERT'), 'web service cannot create administrators');
select ok(not has_table_privilege('service_role', 'public.platform_admins', 'UPDATE'), 'web service cannot elevate administrators');
select ok(not has_table_privilege('service_role', 'public.platform_admins', 'DELETE'), 'web service cannot delete administrators');

insert into auth.users (id, email, email_confirmed_at) values
  ('00000000-0000-0000-0000-000000000021', 'platform-owner@example.test', now()),
  ('00000000-0000-0000-0000-000000000022', 'ordinary-user@example.test', now());

insert into public.platform_admins (user_id, role, created_by)
values ('00000000-0000-0000-0000-000000000021', 'owner', '00000000-0000-0000-0000-000000000021');

select throws_ok(
  $$insert into public.platform_admins (user_id, role, created_by) values ('00000000-0000-0000-0000-000000000022', 'invalid', '00000000-0000-0000-0000-000000000021')$$,
  '23514', null, 'invalid platform role is rejected'
);

set local role authenticated;
set local "request.jwt.claim.sub" = '00000000-0000-0000-0000-000000000021';
select throws_ok(
  $$select count(*) from public.platform_admins$$,
  '42501', 'permission denied for table platform_admins', 'client queries cannot inspect the registry'
);
reset role;

set local role service_role;
select results_eq($$select count(*) from public.platform_admins$$, array[1::bigint], 'server-only service role can read the registry');
insert into public.platform_admin_audit (actor_id, action, target_user_id)
values ('00000000-0000-0000-0000-000000000021', 'accounts.read', '00000000-0000-0000-0000-000000000022');
select results_eq($$select count(*) from public.platform_admin_audit$$, array[1::bigint], 'server-only service role can append audit metadata');
reset role;

select * from finish();
rollback;
