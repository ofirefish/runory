begin;
create extension if not exists pgtap with schema extensions;
set local search_path = public, extensions;
select plan(43);

select ok(
  (select bool_and(class.relrowsecurity)
   from pg_class class
   join pg_namespace namespace on namespace.oid = class.relnamespace
   where namespace.nspname = 'public'
     and class.relname = any (array[
       'billing_plans', 'organization_subscriptions', 'billing_trial_claims', 'credit_accounts',
       'model_price_versions', 'ai_usage_requests', 'credit_holds', 'credit_ledger'
     ])),
  'all billing tables have RLS enabled'
);
select ok(has_table_privilege('anon', 'public.billing_plans', 'SELECT'), 'pricing catalog is public');
select ok(not has_table_privilege('anon', 'public.credit_accounts', 'SELECT'), 'anonymous users cannot read balances');
select ok(has_table_privilege('authenticated', 'public.credit_accounts', 'SELECT'), 'members may read RLS-scoped balances');
select ok(not has_table_privilege('authenticated', 'public.credit_accounts', 'UPDATE'), 'clients cannot mutate balances');
select ok(not has_table_privilege('authenticated', 'public.credit_ledger', 'INSERT'), 'clients cannot write ledger entries');
select ok(not has_function_privilege('authenticated', 'public.billing_reserve_ai_credits(uuid,uuid,uuid,uuid,text,integer,integer)', 'EXECUTE'), 'clients cannot reserve credits directly');
select ok(has_function_privilege('service_role', 'public.billing_reserve_ai_credits(uuid,uuid,uuid,uuid,text,integer,integer)', 'EXECUTE'), 'managed backend can reserve credits');
select ok(not has_function_privilege('authenticated', 'public.billing_apply_credit_grant(uuid,bigint,text,text,text)', 'EXECUTE'), 'clients cannot grant credits');
select results_eq('select count(*) from public.billing_plans', array[4::bigint], 'free and three paid plans are seeded');
select results_eq('select count(*) from public.model_price_versions where retired_at is null', array[2::bigint], 'managed models have active price versions');

insert into auth.users (id, email, email_confirmed_at) values
  ('00000000-0000-0000-0000-000000000011', 'billing-owner@example.test', now()),
  ('00000000-0000-0000-0000-000000000012', 'billing-other@example.test', now());

set local role authenticated;
set local "request.jwt.claim.sub" = '00000000-0000-0000-0000-000000000011';
insert into public.organizations (id, name, owner_id) values (
  '10000000-0000-0000-0000-000000000011', 'Billing workspace',
  '00000000-0000-0000-0000-000000000011'
);
select results_eq(
  $$select balance_microcredits from public.credit_accounts where organization_id = '10000000-0000-0000-0000-000000000011'$$,
  array[0::bigint], 'organization creation provisions an empty credit account'
);
select throws_ok(
  $$update public.credit_accounts set balance_microcredits = 999999 where organization_id = '10000000-0000-0000-0000-000000000011'$$,
  '42501', 'permission denied for table credit_accounts', 'owner cannot forge a balance'
);
reset role;

set local role service_role;
select is(
  public.billing_apply_credit_grant(
    '10000000-0000-0000-0000-000000000011', 10000, 'grant',
    'fixture:welcome:11', 'credits.welcome'
  ), 10000::bigint, 'backend can grant credits exactly once'
);
select is(
  public.billing_apply_credit_grant(
    '10000000-0000-0000-0000-000000000011', 10000, 'grant',
    'fixture:welcome:11', 'credits.welcome'
  ), 10000::bigint, 'duplicate grant reference is idempotent'
);
select lives_ok(
  $$select * from public.billing_reserve_ai_credits(
    '10000000-0000-0000-0000-000000000011',
    '00000000-0000-0000-0000-000000000011',
    '20000000-0000-0000-0000-000000000011',
    '30000000-0000-0000-0000-000000000011',
    'runory-agent-fast', 100, 100
  )$$,
  'backend can reserve the maximum turn cost'
);
select results_eq(
  $$select held_microcredits from public.credit_accounts where organization_id = '10000000-0000-0000-0000-000000000011'$$,
  array[140::bigint], 'reservation reduces spendable credits without charging the ledger'
);
select results_eq(
  $$select count(*) from public.credit_ledger where organization_id = '10000000-0000-0000-0000-000000000011'$$,
  array[1::bigint], 'reservation is not recorded as a financial ledger mutation'
);
select lives_ok(
  $$select * from public.billing_reserve_ai_credits(
    '10000000-0000-0000-0000-000000000011',
    '00000000-0000-0000-0000-000000000011',
    '20000000-0000-0000-0000-000000000099',
    '30000000-0000-0000-0000-000000000011',
    'runory-agent-fast', 100, 100
  )$$,
  'duplicate idempotency key returns the existing reservation'
);
select results_eq(
  $$select count(*) from public.ai_usage_requests where organization_id = '10000000-0000-0000-0000-000000000011'$$,
  array[1::bigint], 'idempotent reserve creates one usage request'
);
select is(
  public.billing_settle_ai_credits('20000000-0000-0000-0000-000000000011', 100, 50),
  125::bigint, 'settlement uses actual provider token usage'
);
select results_eq(
  $$select balance_microcredits, held_microcredits from public.credit_accounts where organization_id = '10000000-0000-0000-0000-000000000011'$$,
  $$values (9875::bigint, 0::bigint)$$,
  'settlement charges actual cost and releases the unused hold'
);
select results_eq(
  $$select amount_microcredits from public.credit_ledger where entry_type = 'charge' and request_id = '20000000-0000-0000-0000-000000000011'$$,
  array[-125::bigint], 'charge is appended to the immutable ledger'
);
select is(
  public.billing_settle_ai_credits('20000000-0000-0000-0000-000000000011', 100, 50),
  125::bigint, 'duplicate settlement is idempotent'
);
select throws_ok(
  $$update public.credit_ledger set amount_microcredits = -1 where request_id = '20000000-0000-0000-0000-000000000011'$$,
  '55000', 'credit ledger is append-only', 'ledger entries cannot be rewritten'
);
select lives_ok(
  $$select * from public.billing_reserve_ai_credits(
    '10000000-0000-0000-0000-000000000011',
    '00000000-0000-0000-0000-000000000011',
    '20000000-0000-0000-0000-000000000012',
    '30000000-0000-0000-0000-000000000012',
    'runory-agent-fast', 100, 100
  )$$,
  'a second turn can be reserved'
);
select lives_ok(
  $$select public.billing_release_ai_credits('20000000-0000-0000-0000-000000000012', 'MODEL_UNAVAILABLE')$$,
  'provider failure releases the reservation'
);
select results_eq(
  $$select held_microcredits from public.credit_accounts where organization_id = '10000000-0000-0000-0000-000000000011'$$,
  array[0::bigint], 'released calls leave no held credits'
);
select results_eq(
  $$select status, error_code from public.ai_usage_requests where id = '20000000-0000-0000-0000-000000000012'$$,
  $$values ('failed'::text, 'MODEL_UNAVAILABLE'::text)$$,
  'released calls retain content-free failure metadata'
);
reset role;

set local role service_role;
select lives_ok(
  $$select * from public.billing_reserve_ai_credits(
    '10000000-0000-0000-0000-000000000011',
    '00000000-0000-0000-0000-000000000011',
    '20000000-0000-0000-0000-000000000013',
    '30000000-0000-0000-0000-000000000013',
    'runory-agent-fast', 100, 100
  )$$,
  'a managed request can create a temporary credit hold'
);
select results_eq(
  $$select held_microcredits > 0 from public.credit_accounts where organization_id = '10000000-0000-0000-0000-000000000011'$$,
  array[true], 'the temporary hold reduces immediately available credits'
);
reset role;
update public.credit_holds set expires_at = now() - interval '1 minute'
where request_id = '20000000-0000-0000-0000-000000000013';
set local role service_role;
select results_eq(
  $$select public.billing_release_expired_ai_holds('10000000-0000-0000-0000-000000000011')$$,
  array[1], 'expired holds are reconciled before the next managed request'
);
select results_eq(
  $$select public.billing_release_expired_ai_holds('10000000-0000-0000-0000-000000000011')$$,
  array[0], 'expired hold reconciliation is idempotent'
);
reset role;
select results_eq(
  $$select status, error_code from public.ai_usage_requests where id = '20000000-0000-0000-0000-000000000013'$$,
  $$values ('failed'::text, 'RESERVATION_EXPIRED'::text)$$,
  'expired usage requests retain content-free failure metadata'
);
select results_eq(
  $$select held_microcredits from public.credit_accounts where organization_id = '10000000-0000-0000-0000-000000000011'$$,
  array[0::bigint], 'expired reconciliation returns held credits to availability'
);

set local role authenticated;
set local "request.jwt.claim.sub" = '00000000-0000-0000-0000-000000000011';
select lives_ok(
  $$select public.start_billing_trial('10000000-0000-0000-0000-000000000011', 'team')$$,
  'team workspace owner can start the one-time trial'
);
select results_eq(
  $$select status, plan_code, seat_quantity from public.organization_subscriptions where organization_id = '10000000-0000-0000-0000-000000000011'$$,
  $$values ('trialing'::text, 'team'::text, 1)$$,
  'trial creates an organization subscription for the current seat count'
);
select results_eq(
  $$select balance_microcredits from public.credit_accounts where organization_id = '10000000-0000-0000-0000-000000000011'$$,
  array[2509875::bigint], 'trial grants ten percent of monthly plan credits'
);
select throws_ok(
  $$select public.start_billing_trial('10000000-0000-0000-0000-000000000011', 'business')$$,
  '23505', 'trial already claimed', 'an account cannot claim repeated organization trials'
);
reset role;
select results_eq(
  $$select count(*) from public.billing_trial_claims where organization_id = '10000000-0000-0000-0000-000000000011'$$,
  array[1::bigint], 'trial claim is recorded once'
);
select results_eq(
  $$select count(*) from public.credit_ledger where organization_id = '10000000-0000-0000-0000-000000000011' and description_code = 'credits.trial'$$,
  array[1::bigint], 'trial credit grant is appended once'
);
reset role;

set local role authenticated;
set local "request.jwt.claim.sub" = '00000000-0000-0000-0000-000000000012';
select results_eq(
  $$select count(*) from public.credit_accounts where organization_id = '10000000-0000-0000-0000-000000000011'$$,
  array[0::bigint], 'non-member cannot read another organization balance'
);
select results_eq(
  $$select count(*) from public.credit_ledger where organization_id = '10000000-0000-0000-0000-000000000011'$$,
  array[0::bigint], 'non-member cannot read another organization ledger'
);
reset role;

select * from finish();
rollback;
