create or replace function public.write_sync_object(
  target_organization_id uuid,
  target_kind text,
  target_logical_id uuid,
  target_encrypted_payload jsonb,
  expected_revision bigint
)
returns public.sync_objects
language plpgsql
security definer
set search_path = ''
as $$
declare
  written public.sync_objects;
  caller_id uuid := (select auth.uid());
begin
  if caller_id is null
    or coalesce((select private.organization_role(target_organization_id)), '') not in ('owner', 'admin', 'operator')
  then
    raise exception 'sync write is unavailable' using errcode = '42501';
  end if;

  if expected_revision < 0
    or target_kind not in ('profile', 'group', 'inventory', 'known-host')
    or jsonb_typeof(target_encrypted_payload) <> 'object'
    or octet_length(target_encrypted_payload::text) > 12000000
  then
    raise exception 'invalid sync object' using errcode = '22023';
  end if;

  if expected_revision = 0 then
    insert into public.sync_objects (
      organization_id, kind, logical_id, encrypted_payload, revision, updated_by
    ) values (
      target_organization_id, target_kind, target_logical_id,
      target_encrypted_payload, 1, caller_id
    )
    on conflict (organization_id, kind, logical_id) do nothing
    returning * into written;
  else
    update public.sync_objects
    set encrypted_payload = target_encrypted_payload,
        revision = revision + 1,
        updated_by = caller_id,
        updated_at = now()
    where organization_id = target_organization_id
      and kind = target_kind
      and logical_id = target_logical_id
      and revision = expected_revision
    returning * into written;
  end if;

  if written.id is null then
    raise exception 'sync revision conflict' using errcode = '40001';
  end if;

  insert into public.audit_records (
    organization_id, actor_id, action, resource_type, resource_id, result
  ) values (
    target_organization_id, caller_id, 'sync.write', 'sync-object',
    written.id::text, 'succeeded'
  );
  return written;
end;
$$;

comment on function public.write_sync_object(uuid, text, uuid, jsonb, bigint) is
  'Atomically creates or updates an encrypted sync object with optimistic revision checking.';

create or replace function public.accept_organization_invite(target_invite_id uuid)
returns uuid
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller_id uuid := (select auth.uid());
  caller_email text;
  invitation public.organization_invites;
begin
  if caller_id is null then
    raise exception 'invitation is unavailable' using errcode = '42501';
  end if;

  select lower(email) into caller_email
  from auth.users
  where id = caller_id and email_confirmed_at is not null;

  select * into invitation
  from public.organization_invites
  where id = target_invite_id
    and accepted_at is null
    and expires_at > now()
    and lower(email) = caller_email
  for update;

  if invitation.id is null then
    raise exception 'invitation is unavailable' using errcode = '42501';
  end if;

  insert into public.organization_members (organization_id, user_id, role)
  values (invitation.organization_id, caller_id, invitation.role)
  on conflict (organization_id, user_id) do nothing;

  update public.organization_invites
  set accepted_at = now()
  where id = invitation.id;

  insert into public.audit_records (
    organization_id, actor_id, action, resource_type, resource_id, result
  ) values (
    invitation.organization_id, caller_id, 'invite.accept', 'organization-invite',
    invitation.id::text, 'succeeded'
  );
  return invitation.organization_id;
end;
$$;

comment on function public.accept_organization_invite(uuid) is
  'Accepts only an unexpired invitation matching the authenticated user verified email.';

revoke all on function public.write_sync_object(uuid, text, uuid, jsonb, bigint) from public, anon;
revoke all on function public.accept_organization_invite(uuid) from public, anon;
grant execute on function public.write_sync_object(uuid, text, uuid, jsonb, bigint) to authenticated;
grant execute on function public.accept_organization_invite(uuid) to authenticated;

-- All sync mutations pass through the revision-checking function above.
revoke insert, update on table public.sync_objects from authenticated;
