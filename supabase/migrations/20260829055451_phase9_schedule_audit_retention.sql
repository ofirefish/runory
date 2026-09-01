create extension if not exists pg_cron with schema pg_catalog;

create index audit_records_retention_idx
on public.audit_records (occurred_at, id);

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

  with expired as materialized (
    select record.id
    from public.audit_records record
    where record.occurred_at < now() - make_interval(days => retention_days)
    order by record.occurred_at, record.id
    limit 10000
    for update skip locked
  ), deleted as (
    delete from public.audit_records record
    using expired
    where record.id = expired.id
    returning record.id
  )
  select count(*) into removed from deleted;

  return removed;
end;
$$;

revoke all on function private.prune_audit_records(integer) from public, anon, authenticated;

comment on function private.prune_audit_records(integer) is
  'Privileged bounded audit retention batch. Scheduled daily by Supabase Cron.';

select cron.schedule(
  'runory-audit-retention-daily',
  '17 3 * * *',
  $$select private.prune_audit_records(180);$$
);

-- Extension objects are owned by Supabase's managed role. The schema boundary prevents Data API
-- roles from reaching their internal PUBLIC ACL without altering managed-role ownership.
revoke all on schema cron from public, anon, authenticated;
