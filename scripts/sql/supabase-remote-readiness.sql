with expected_tables(table_name) as (
  values
    ('organizations'),
    ('organization_members'),
    ('organization_invites'),
    ('sync_objects'),
    ('access_policies'),
    ('audit_records')
), migration_state as (
  select
    count(*)::integer as migration_count,
    coalesce(json_agg(version order by version)::text, '[]') as migration_versions_json
  from supabase_migrations.schema_migrations
), cron_state as (
  select jobid, schedule, command, active
  from cron.job
  where jobname = 'runory-audit-retention-daily'
), cron_run as (
  select details.status, details.start_time, details.end_time
  from cron.job_run_details details
  join cron_state job on job.jobid = details.jobid
  order by details.start_time desc
  limit 1
)
select
  current_setting('server_version_num')::integer >= 170000 as postgres_17_or_newer,
  migration_state.migration_count,
  migration_state.migration_versions_json,
  not exists (
    select 1
    from expected_tables expected
    left join pg_class relation
      on relation.relname = expected.table_name
      and relation.relnamespace = 'public'::regnamespace
      and relation.relkind = 'r'
    where relation.oid is null or not relation.relrowsecurity
  ) as all_business_tables_have_rls,
  has_function_privilege(
    'authenticated',
    'public.evaluate_access_policy(uuid,text,text,uuid)',
    'EXECUTE'
  ) and not has_function_privilege(
    'anon',
    'public.evaluate_access_policy(uuid,text,text,uuid)',
    'EXECUTE'
  ) as policy_rpc_acl_valid,
  to_regprocedure('private.prune_audit_records(integer)') is not null
    and not has_function_privilege(
      'authenticated',
      'private.prune_audit_records(integer)',
      'EXECUTE'
    ) as retention_function_private,
  exists (
    select 1 from pg_indexes
    where schemaname = 'public'
      and tablename = 'audit_records'
      and indexname = 'audit_records_retention_idx'
  ) as retention_index_present,
  (select count(*) = 1 from cron_state) as audit_cron_unique,
  coalesce((select schedule = '17 3 * * *' from cron_state), false) as audit_cron_schedule_valid,
  coalesce((select active from cron_state), false) as audit_cron_active,
  coalesce(
    (select position('private.prune_audit_records(180)' in command) > 0 from cron_state),
    false
  ) as audit_cron_command_valid,
  exists (
    select 1
    from cron.job_run_details details
    join cron_state job on job.jobid = details.jobid
    where details.status = 'succeeded'
  ) as audit_cron_has_successful_run,
  coalesce((select status = 'succeeded' from cron_run), false) as audit_cron_latest_succeeded,
  (select status from cron_run) as audit_cron_latest_status,
  (select start_time from cron_run) as audit_cron_latest_start,
  (select end_time from cron_run) as audit_cron_latest_end
from migration_state;
