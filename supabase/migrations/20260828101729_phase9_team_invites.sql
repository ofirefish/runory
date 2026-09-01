create or replace function public.create_organization_invite(
  target_organization_id uuid,
  target_email text,
  target_role text
)
returns public.organization_invites
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller_id uuid := (select auth.uid());
  normalized_email text := lower(btrim(target_email));
  invitation public.organization_invites;
begin
  if caller_id is null
    or coalesce((select private.organization_role(target_organization_id)), '') not in ('owner', 'admin')
  then
    raise exception 'invite creation is unavailable' using errcode = '42501';
  end if;
  if target_role not in ('admin', 'operator', 'viewer')
    or char_length(normalized_email) not between 3 and 320
    or position('@' in normalized_email) < 2
  then
    raise exception 'invalid invitation' using errcode = '22023';
  end if;

  insert into public.organization_invites (
    organization_id, email, role, invited_by, expires_at
  ) values (
    target_organization_id, normalized_email, target_role, caller_id, now() + interval '7 days'
  )
  on conflict (organization_id, email) do update
  set role = excluded.role,
      invited_by = excluded.invited_by,
      expires_at = excluded.expires_at,
      accepted_at = null,
      created_at = now()
  returning * into invitation;

  insert into public.audit_records (
    organization_id, actor_id, action, resource_type, resource_id, result
  ) values (
    target_organization_id, caller_id, 'invite.create', 'organization-invite',
    invitation.id::text, 'succeeded'
  );
  return invitation;
end;
$$;

create or replace function public.list_my_organization_invites()
returns table (
  id uuid,
  organization_id uuid,
  organization_name text,
  email text,
  role text,
  expires_at timestamptz,
  created_at timestamptz
)
language sql
stable
security definer
set search_path = ''
as $$
  select invite.id, invite.organization_id, organization.name, invite.email,
         invite.role, invite.expires_at, invite.created_at
  from public.organization_invites invite
  join public.organizations organization on organization.id = invite.organization_id
  join auth.users account on account.id = (select auth.uid())
  where account.email_confirmed_at is not null
    and lower(invite.email) = lower(account.email)
    and invite.accepted_at is null
    and invite.expires_at > now()
  order by invite.created_at desc;
$$;

create or replace function public.list_organization_members(target_organization_id uuid)
returns table (
  user_id uuid,
  email text,
  role text,
  created_at timestamptz
)
language plpgsql
stable
security definer
set search_path = ''
as $$
begin
  if (select auth.uid()) is null
    or coalesce((select private.organization_role(target_organization_id)), '') not in ('owner', 'admin')
  then
    raise exception 'member list is unavailable' using errcode = '42501';
  end if;
  return query
    select member.user_id, account.email::text, member.role, member.created_at
    from public.organization_members member
    join auth.users account on account.id = member.user_id
    where member.organization_id = target_organization_id
    order by member.created_at;
end;
$$;

create or replace function public.revoke_organization_invite(target_invite_id uuid)
returns void
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller_id uuid := (select auth.uid());
  invitation public.organization_invites;
begin
  select * into invitation
  from public.organization_invites
  where id = target_invite_id
  for update;
  if invitation.id is null
    or caller_id is null
    or coalesce((select private.organization_role(invitation.organization_id)), '') not in ('owner', 'admin')
  then
    raise exception 'invitation is unavailable' using errcode = '42501';
  end if;
  delete from public.organization_invites where id = invitation.id;
  insert into public.audit_records (
    organization_id, actor_id, action, resource_type, resource_id, result
  ) values (
    invitation.organization_id, caller_id, 'invite.revoke', 'organization-invite',
    invitation.id::text, 'succeeded'
  );
end;
$$;

revoke all on function public.create_organization_invite(uuid, text, text) from public, anon;
revoke all on function public.list_my_organization_invites() from public, anon;
revoke all on function public.list_organization_members(uuid) from public, anon;
revoke all on function public.revoke_organization_invite(uuid) from public, anon;
grant execute on function public.create_organization_invite(uuid, text, text) to authenticated;
grant execute on function public.list_my_organization_invites() to authenticated;
grant execute on function public.list_organization_members(uuid) to authenticated;
grant execute on function public.revoke_organization_invite(uuid) to authenticated;

-- Invitation creation is centralized so expiry, normalization and audit cannot be bypassed.
revoke insert, update, delete on table public.organization_invites from authenticated;
