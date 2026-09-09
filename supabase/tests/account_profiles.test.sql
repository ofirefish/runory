begin;
create extension if not exists pgtap with schema extensions;
set local search_path = public, extensions;
select plan(29);

select ok(
  (select relrowsecurity from pg_class where oid = 'public.user_profiles'::regclass),
  'account profiles have RLS enabled'
);
select ok(not has_table_privilege('anon', 'public.user_profiles', 'SELECT'), 'anon cannot read account profiles');
select ok(has_table_privilege('authenticated', 'public.user_profiles', 'SELECT'), 'authenticated may read RLS-visible profiles');
select ok(not has_table_privilege('authenticated', 'public.user_profiles', 'INSERT'), 'clients cannot bypass profile creation RPC');
select ok(not has_table_privilege('authenticated', 'public.user_profiles', 'UPDATE'), 'clients cannot bypass profile update RPCs');
select ok(not has_table_privilege('authenticated', 'public.user_profiles', 'DELETE'), 'clients cannot delete account profiles');
select ok(not has_function_privilege('anon', 'public.ensure_personal_workspace()', 'EXECUTE'), 'anon cannot create a personal workspace');
select ok(has_function_privilege('authenticated', 'public.ensure_personal_workspace()', 'EXECUTE'), 'authenticated can ensure a personal workspace');
select ok(not has_function_privilege('anon', 'public.ensure_my_profile(text)', 'EXECUTE'), 'anon cannot create an account profile');
select ok(has_function_privilege('authenticated', 'public.ensure_my_profile(text)', 'EXECUTE'), 'authenticated can ensure their account profile');
select ok(not has_function_privilege('anon', 'public.set_my_avatar(text,bigint)', 'EXECUTE'), 'anon cannot update an avatar');

select ok(
  exists (select 1 from storage.buckets where id = 'avatars' and not public),
  'avatars bucket is private'
);
select is(
  (select file_size_limit from storage.buckets where id = 'avatars'),
  2097152::bigint,
  'avatars are limited to two MiB'
);
select ok(
  (select allowed_mime_types @> array['image/jpeg', 'image/png', 'image/webp']::text[]
   from storage.buckets where id = 'avatars'),
  'avatars allow only the expected image formats'
);
select ok(exists (select 1 from pg_policies where schemaname = 'storage' and tablename = 'objects' and policyname = 'avatars_select'), 'avatar select policy exists');
select ok(exists (select 1 from pg_policies where schemaname = 'storage' and tablename = 'objects' and policyname = 'avatars_insert'), 'avatar insert policy exists');
select ok(exists (select 1 from pg_policies where schemaname = 'storage' and tablename = 'objects' and policyname = 'avatars_delete'), 'avatar delete policy exists');
select ok(not exists (select 1 from pg_policies where schemaname = 'storage' and tablename = 'objects' and policyname = 'avatars_update'), 'avatar overwrite policy is absent');

insert into auth.users (id, email, email_confirmed_at) values
  ('00000000-0000-0000-0000-000000000011', 'alice@example.test', now()),
  ('00000000-0000-0000-0000-000000000012', 'bob@example.test', now()),
  ('00000000-0000-0000-0000-000000000013', 'carol@example.test', now());

set local role authenticated;
set local "request.jwt.claim.sub" = '00000000-0000-0000-0000-000000000011';
select is((public.ensure_personal_workspace()).kind, 'personal', 'verified user creates a personal workspace');
select public.ensure_personal_workspace();
select results_eq(
  $$select count(*) from public.organizations where owner_id = '00000000-0000-0000-0000-000000000011' and kind = 'personal'$$,
  array[1::bigint],
  'personal workspace creation is idempotent'
);
select is((public.ensure_my_profile('Alice')).display_name, 'Alice', 'user creates their profile');
select is((public.update_my_display_name('Alice Updated')).display_name, 'Alice Updated', 'user updates their display name');
select throws_ok(
  $$select public.set_my_avatar('00000000-0000-0000-0000-000000000012/10000000-0000-0000-0000-000000000001.webp', 0)$$,
  '22023', 'invalid avatar update', 'avatar path cannot target another account'
);
select is(
  (public.set_my_avatar('00000000-0000-0000-0000-000000000011/10000000-0000-0000-0000-000000000001.webp', 0)).avatar_version,
  1::bigint,
  'avatar updates use an owned immutable path'
);
select throws_ok(
  $$select public.set_my_avatar(null, 0)$$,
  '40001', 'avatar revision conflict', 'stale avatar revision fails closed'
);
insert into public.organizations (id, name, owner_id, kind) values (
  '10000000-0000-0000-0000-000000000011', 'Team A',
  '00000000-0000-0000-0000-000000000011', 'team'
);
reset role;

insert into public.organization_members (organization_id, user_id, role) values (
  '10000000-0000-0000-0000-000000000011',
  '00000000-0000-0000-0000-000000000012', 'viewer'
);

set local role authenticated;
set local "request.jwt.claim.sub" = '00000000-0000-0000-0000-000000000012';
select public.ensure_my_profile('Bob');
reset role;

set local role authenticated;
set local "request.jwt.claim.sub" = '00000000-0000-0000-0000-000000000011';
select results_eq(
  $$select count(*) from public.user_profiles where id = '00000000-0000-0000-0000-000000000012'$$,
  array[1::bigint],
  'members can read profiles in a shared organization'
);
select throws_ok(
  $$select public.create_organization_invite((select id from public.organizations where kind = 'personal'), 'invitee@example.test', 'viewer')$$,
  '42501', 'personal workspace invitations are unavailable',
  'personal workspaces reject invitations'
);
select throws_ok(
  $$update public.organizations set kind = 'personal' where id = '10000000-0000-0000-0000-000000000011'$$,
  '22023', 'organization kind is immutable', 'workspace kind cannot be changed'
);
reset role;

set local role authenticated;
set local "request.jwt.claim.sub" = '00000000-0000-0000-0000-000000000013';
select results_eq(
  $$select count(*) from public.user_profiles where id = '00000000-0000-0000-0000-000000000012'$$,
  array[0::bigint],
  'unrelated accounts cannot read a profile'
);
reset role;

select * from finish();
rollback;
