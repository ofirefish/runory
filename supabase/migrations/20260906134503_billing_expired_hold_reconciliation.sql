create or replace function public.billing_release_expired_ai_holds(
  target_organization_id uuid
)
returns integer
language plpgsql
set search_path = ''
as $$
declare
  expired_hold record;
  released_count integer := 0;
begin
  for expired_hold in
    select hold.request_id
    from public.credit_holds hold
    join public.ai_usage_requests usage on usage.id = hold.request_id
    where hold.organization_id = target_organization_id
      and hold.status = 'held'
      and hold.expires_at <= now()
      and usage.status = 'reserved'
    order by hold.request_id
    for update of hold
  loop
    perform public.billing_release_ai_credits(expired_hold.request_id, 'RESERVATION_EXPIRED');
    released_count := released_count + 1;
  end loop;
  return released_count;
end;
$$;

revoke all on function public.billing_release_expired_ai_holds(uuid) from public, anon, authenticated;
grant execute on function public.billing_release_expired_ai_holds(uuid) to service_role;
