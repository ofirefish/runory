create table public.billing_plans (
  code text primary key check (code in ('pro', 'team', 'business')),
  monthly_price_cents integer not null check (monthly_price_cents >= 0),
  annual_monthly_price_cents integer not null check (annual_monthly_price_cents >= 0),
  currency text not null default 'USD' check (currency = 'USD'),
  per_seat boolean not null,
  trial_days integer not null default 0 check (trial_days between 0 and 90),
  included_monthly_credits bigint not null check (included_monthly_credits >= 0),
  feature_codes jsonb not null default '[]'::jsonb check (jsonb_typeof(feature_codes) = 'array'),
  sort_order smallint not null unique,
  active boolean not null default true,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

insert into public.billing_plans (
  code, monthly_price_cents, annual_monthly_price_cents, currency, per_seat,
  trial_days, included_monthly_credits, feature_codes, sort_order
) values
  (
    'pro', 1200, 1000, 'USD', false, 0, 10000,
    '["cloud-sync","personal-vault","managed-ai","usage-history"]'::jsonb, 1
  ),
  (
    'team', 2400, 2000, 'USD', true, 14, 25000,
    '["pro-features","team-workspaces","role-management","consolidated-billing"]'::jsonb, 2
  ),
  (
    'business', 3600, 3000, 'USD', true, 14, 50000,
    '["team-features","access-policies","audit-history","priority-ai"]'::jsonb, 3
  );

create table public.organization_subscriptions (
  organization_id uuid primary key references public.organizations(id) on delete cascade,
  plan_code text not null references public.billing_plans(code) on delete restrict,
  status text not null check (status in ('trialing', 'active', 'past-due', 'cancelled', 'expired')),
  seat_quantity integer not null default 1 check (seat_quantity between 1 and 100000),
  billing_cycle text not null check (billing_cycle in ('monthly', 'annual')),
  current_period_start timestamptz not null,
  current_period_end timestamptz not null,
  trial_ends_at timestamptz,
  trial_started_at timestamptz,
  cancel_at_period_end boolean not null default false,
  provider_customer_ref text,
  provider_subscription_ref text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  check (current_period_end > current_period_start),
  check (provider_customer_ref is null or char_length(provider_customer_ref) between 1 and 200),
  check (provider_subscription_ref is null or char_length(provider_subscription_ref) between 1 and 200)
);

create table public.billing_trial_claims (
  actor_id uuid primary key references auth.users(id) on delete restrict,
  organization_id uuid not null references public.organizations(id) on delete cascade,
  plan_code text not null references public.billing_plans(code) on delete restrict,
  claimed_at timestamptz not null default now()
);

create index billing_trial_claims_organization_id_idx
on public.billing_trial_claims (organization_id);

create unique index organization_subscriptions_provider_ref_idx
on public.organization_subscriptions (provider_subscription_ref)
where provider_subscription_ref is not null;

create table public.credit_accounts (
  organization_id uuid primary key references public.organizations(id) on delete cascade,
  balance_microcredits bigint not null default 0 check (balance_microcredits >= 0),
  held_microcredits bigint not null default 0 check (held_microcredits >= 0),
  lifetime_granted_microcredits bigint not null default 0 check (lifetime_granted_microcredits >= 0),
  lifetime_spent_microcredits bigint not null default 0 check (lifetime_spent_microcredits >= 0),
  version bigint not null default 1 check (version > 0),
  updated_at timestamptz not null default now(),
  check (held_microcredits <= balance_microcredits)
);

create table public.model_price_versions (
  id bigint generated always as identity primary key,
  public_model_id text not null check (public_model_id ~ '^[a-z0-9][a-z0-9-]{1,63}$'),
  provider text not null check (provider in ('deepseek', 'glm')),
  provider_model_id text not null check (char_length(provider_model_id) between 1 and 128),
  input_microcredits_per_million bigint not null check (input_microcredits_per_million >= 0),
  output_microcredits_per_million bigint not null check (output_microcredits_per_million >= 0),
  request_fee_microcredits bigint not null default 0 check (request_fee_microcredits >= 0),
  max_context_tokens integer not null check (max_context_tokens between 256 and 1000000),
  max_output_tokens integer not null check (max_output_tokens between 1 and 65536),
  effective_at timestamptz not null default now(),
  retired_at timestamptz,
  created_at timestamptz not null default now(),
  check (retired_at is null or retired_at > effective_at)
);

create unique index model_price_versions_one_active_idx
on public.model_price_versions (public_model_id)
where retired_at is null;

insert into public.model_price_versions (
  public_model_id, provider, provider_model_id,
  input_microcredits_per_million, output_microcredits_per_million,
  request_fee_microcredits, max_context_tokens, max_output_tokens
) values
  ('runory-agent-fast', 'deepseek', 'deepseek-v4-flash', 100000, 300000, 100, 131072, 4096),
  ('runory-agent-pro', 'glm', 'glm-5.2', 300000, 900000, 200, 131072, 8192);

create table public.ai_usage_requests (
  id uuid primary key,
  organization_id uuid not null references public.organizations(id) on delete cascade,
  actor_id uuid not null references auth.users(id) on delete restrict,
  idempotency_key uuid not null,
  public_model_id text not null,
  price_version_id bigint not null references public.model_price_versions(id) on delete restrict,
  provider text not null,
  provider_model_id text not null,
  status text not null check (status in ('reserved', 'succeeded', 'failed', 'settlement-pending')),
  estimated_input_tokens integer not null check (estimated_input_tokens >= 0),
  max_output_tokens integer not null check (max_output_tokens > 0),
  actual_input_tokens integer check (actual_input_tokens is null or actual_input_tokens >= 0),
  actual_output_tokens integer check (actual_output_tokens is null or actual_output_tokens >= 0),
  reserved_microcredits bigint not null check (reserved_microcredits >= 0),
  charged_microcredits bigint not null default 0 check (charged_microcredits >= 0),
  error_code text check (error_code is null or char_length(error_code) between 1 and 64),
  created_at timestamptz not null default now(),
  settled_at timestamptz,
  unique (organization_id, idempotency_key)
);

create index ai_usage_requests_organization_time_idx
on public.ai_usage_requests (organization_id, created_at desc);
create index ai_usage_requests_actor_id_idx on public.ai_usage_requests (actor_id);

create table public.credit_holds (
  request_id uuid primary key references public.ai_usage_requests(id) on delete restrict,
  organization_id uuid not null references public.organizations(id) on delete cascade,
  amount_microcredits bigint not null check (amount_microcredits > 0),
  status text not null check (status in ('held', 'captured', 'released', 'expired')),
  expires_at timestamptz not null,
  created_at timestamptz not null default now(),
  released_at timestamptz
);

create index credit_holds_organization_status_idx
on public.credit_holds (organization_id, status, expires_at);

create table public.credit_ledger (
  id bigint generated always as identity primary key,
  organization_id uuid not null references public.organizations(id) on delete cascade,
  actor_id uuid references auth.users(id) on delete restrict,
  request_id uuid references public.ai_usage_requests(id) on delete restrict,
  entry_type text not null check (entry_type in ('grant', 'purchase', 'charge', 'refund', 'expiry', 'adjustment')),
  amount_microcredits bigint not null check (amount_microcredits <> 0),
  balance_after_microcredits bigint not null check (balance_after_microcredits >= 0),
  external_reference text,
  description_code text not null check (char_length(description_code) between 1 and 100),
  created_at timestamptz not null default now(),
  check (
    (entry_type in ('grant', 'purchase', 'refund') and amount_microcredits > 0)
    or (entry_type in ('charge', 'expiry') and amount_microcredits < 0)
    or entry_type = 'adjustment'
  )
);

create index credit_ledger_organization_time_idx
on public.credit_ledger (organization_id, created_at desc, id desc);
create unique index credit_ledger_external_reference_idx
on public.credit_ledger (organization_id, external_reference)
where external_reference is not null;
create unique index credit_ledger_request_charge_idx
on public.credit_ledger (request_id)
where entry_type = 'charge';

create or replace function private.create_credit_account()
returns trigger
language plpgsql
security definer
set search_path = ''
as $$
begin
  insert into public.credit_accounts (organization_id) values (new.id)
  on conflict (organization_id) do nothing;
  return new;
end;
$$;

create trigger organizations_create_credit_account
after insert on public.organizations
for each row execute function private.create_credit_account();

insert into public.credit_accounts (organization_id)
select id from public.organizations
on conflict (organization_id) do nothing;

create or replace function private.reject_credit_ledger_mutation()
returns trigger
language plpgsql
set search_path = ''
as $$
begin
  raise exception 'credit ledger is append-only' using errcode = '55000';
end;
$$;

create trigger credit_ledger_reject_mutation
before update or delete on public.credit_ledger
for each row execute function private.reject_credit_ledger_mutation();

create or replace function public.billing_reserve_ai_credits(
  target_organization_id uuid,
  target_actor_id uuid,
  target_request_id uuid,
  target_idempotency_key uuid,
  target_public_model_id text,
  target_estimated_input_tokens integer,
  target_max_output_tokens integer
)
returns table (
  request_id uuid,
  provider text,
  provider_model_id text,
  reserved_microcredits bigint,
  max_output_tokens integer,
  request_status text,
  newly_created boolean
)
language plpgsql
set search_path = ''
as $$
declare
  selected_price public.model_price_versions;
  selected_account public.credit_accounts;
  required_microcredits bigint;
  existing_request public.ai_usage_requests;
begin
  if target_estimated_input_tokens < 0 or target_estimated_input_tokens > 1000000
     or target_max_output_tokens < 1 or target_max_output_tokens > 65536 then
    raise exception 'invalid usage bounds' using errcode = '22023';
  end if;
  if not exists (
    select 1 from public.organization_members member
    where member.organization_id = target_organization_id and member.user_id = target_actor_id
  ) then
    raise exception 'organization forbidden' using errcode = '42501';
  end if;

  select * into existing_request
  from public.ai_usage_requests usage
  where usage.organization_id = target_organization_id
    and usage.idempotency_key = target_idempotency_key;
  if found then
    return query select existing_request.id, existing_request.provider,
      existing_request.provider_model_id, existing_request.reserved_microcredits,
      existing_request.max_output_tokens, existing_request.status, false;
    return;
  end if;

  select * into selected_price
  from public.model_price_versions price
  where price.public_model_id = target_public_model_id
    and price.effective_at <= now() and price.retired_at is null;
  if not found or target_max_output_tokens > selected_price.max_output_tokens
     or target_estimated_input_tokens + target_max_output_tokens > selected_price.max_context_tokens then
    raise exception 'model unavailable' using errcode = '22023';
  end if;

  required_microcredits := selected_price.request_fee_microcredits
    + ((target_estimated_input_tokens::bigint * selected_price.input_microcredits_per_million + 999999) / 1000000)
    + ((target_max_output_tokens::bigint * selected_price.output_microcredits_per_million + 999999) / 1000000);
  required_microcredits := greatest(required_microcredits, 1);

  select * into selected_account from public.credit_accounts account
  where account.organization_id = target_organization_id for update;
  if not found or selected_account.balance_microcredits - selected_account.held_microcredits < required_microcredits then
    raise exception 'insufficient credits' using errcode = 'P0001';
  end if;

  update public.credit_accounts account set
    held_microcredits = account.held_microcredits + required_microcredits,
    version = account.version + 1,
    updated_at = now()
  where account.organization_id = target_organization_id;

  insert into public.ai_usage_requests (
    id, organization_id, actor_id, idempotency_key, public_model_id,
    price_version_id, provider, provider_model_id, status,
    estimated_input_tokens, max_output_tokens, reserved_microcredits
  ) values (
    target_request_id, target_organization_id, target_actor_id, target_idempotency_key,
    target_public_model_id, selected_price.id, selected_price.provider,
    selected_price.provider_model_id, 'reserved', target_estimated_input_tokens,
    target_max_output_tokens, required_microcredits
  );
  insert into public.credit_holds (
    request_id, organization_id, amount_microcredits, status, expires_at
  ) values (
    target_request_id, target_organization_id, required_microcredits, 'held', now() + interval '10 minutes'
  );

  return query select target_request_id, selected_price.provider,
    selected_price.provider_model_id, required_microcredits, target_max_output_tokens,
    'reserved'::text, true;
end;
$$;

create or replace function public.billing_settle_ai_credits(
  target_request_id uuid,
  target_input_tokens integer,
  target_output_tokens integer
)
returns bigint
language plpgsql
set search_path = ''
as $$
declare
  selected_request public.ai_usage_requests;
  selected_price public.model_price_versions;
  selected_account public.credit_accounts;
  charge_microcredits bigint;
begin
  if target_input_tokens < 0 or target_output_tokens < 0 then
    raise exception 'invalid actual usage' using errcode = '22023';
  end if;
  select * into selected_request from public.ai_usage_requests usage
  where usage.id = target_request_id for update;
  if not found then raise exception 'usage request not found' using errcode = 'P0002'; end if;
  if selected_request.status = 'succeeded' then return selected_request.charged_microcredits; end if;
  if selected_request.status <> 'reserved' then
    raise exception 'usage request is not reservable' using errcode = '55000';
  end if;
  select * into selected_price from public.model_price_versions price
  where price.id = selected_request.price_version_id;
  charge_microcredits := selected_price.request_fee_microcredits
    + ((target_input_tokens::bigint * selected_price.input_microcredits_per_million + 999999) / 1000000)
    + ((target_output_tokens::bigint * selected_price.output_microcredits_per_million + 999999) / 1000000);
  charge_microcredits := greatest(charge_microcredits, 1);
  if charge_microcredits > selected_request.reserved_microcredits then
    update public.ai_usage_requests set status = 'settlement-pending', error_code = 'USAGE_EXCEEDS_RESERVATION'
    where id = target_request_id;
    return -1;
  end if;

  select * into selected_account from public.credit_accounts account
  where account.organization_id = selected_request.organization_id for update;
  update public.credit_accounts account set
    balance_microcredits = account.balance_microcredits - charge_microcredits,
    held_microcredits = account.held_microcredits - selected_request.reserved_microcredits,
    lifetime_spent_microcredits = account.lifetime_spent_microcredits + charge_microcredits,
    version = account.version + 1,
    updated_at = now()
  where account.organization_id = selected_request.organization_id;
  insert into public.credit_ledger (
    organization_id, actor_id, request_id, entry_type, amount_microcredits,
    balance_after_microcredits, description_code
  ) values (
    selected_request.organization_id, selected_request.actor_id, target_request_id,
    'charge', -charge_microcredits, selected_account.balance_microcredits - charge_microcredits,
    'credits.agentTurn'
  );
  update public.credit_holds set status = 'captured', released_at = now()
  where request_id = target_request_id and status = 'held';
  update public.ai_usage_requests set
    status = 'succeeded', actual_input_tokens = target_input_tokens,
    actual_output_tokens = target_output_tokens, charged_microcredits = charge_microcredits,
    settled_at = now(), error_code = null
  where id = target_request_id;
  return charge_microcredits;
end;
$$;

create or replace function public.billing_release_ai_credits(
  target_request_id uuid,
  target_error_code text
)
returns void
language plpgsql
set search_path = ''
as $$
declare
  selected_request public.ai_usage_requests;
begin
  if target_error_code is null or char_length(target_error_code) not between 1 and 64 then
    raise exception 'invalid error code' using errcode = '22023';
  end if;
  select * into selected_request from public.ai_usage_requests usage
  where usage.id = target_request_id for update;
  if not found or selected_request.status = 'failed' then return; end if;
  if selected_request.status <> 'reserved' then
    raise exception 'usage request cannot be released' using errcode = '55000';
  end if;
  update public.credit_accounts account set
    held_microcredits = account.held_microcredits - selected_request.reserved_microcredits,
    version = account.version + 1,
    updated_at = now()
  where account.organization_id = selected_request.organization_id;
  update public.credit_holds set status = 'released', released_at = now()
  where request_id = target_request_id and status = 'held';
  update public.ai_usage_requests set status = 'failed', error_code = target_error_code, settled_at = now()
  where id = target_request_id;
end;
$$;

create or replace function public.billing_apply_credit_grant(
  target_organization_id uuid,
  target_amount_microcredits bigint,
  target_entry_type text,
  target_external_reference text,
  target_description_code text
)
returns bigint
language plpgsql
set search_path = ''
as $$
declare
  selected_account public.credit_accounts;
  resulting_balance bigint;
begin
  if target_amount_microcredits <= 0 or target_entry_type not in ('grant', 'purchase', 'refund')
     or target_external_reference is null or char_length(target_external_reference) not between 1 and 200
     or char_length(target_description_code) not between 1 and 100 then
    raise exception 'invalid credit grant' using errcode = '22023';
  end if;
  select * into selected_account from public.credit_accounts account
  where account.organization_id = target_organization_id for update;
  if not found then raise exception 'credit account not found' using errcode = 'P0002'; end if;
  if exists (
    select 1 from public.credit_ledger ledger
    where ledger.organization_id = target_organization_id
      and ledger.external_reference = target_external_reference
  ) then
    return selected_account.balance_microcredits;
  end if;
  resulting_balance := selected_account.balance_microcredits + target_amount_microcredits;
  update public.credit_accounts account set
    balance_microcredits = resulting_balance,
    lifetime_granted_microcredits = account.lifetime_granted_microcredits + target_amount_microcredits,
    version = account.version + 1,
    updated_at = now()
  where account.organization_id = target_organization_id;
  insert into public.credit_ledger (
    organization_id, entry_type, amount_microcredits, balance_after_microcredits,
    external_reference, description_code
  ) values (
    target_organization_id, target_entry_type, target_amount_microcredits,
    resulting_balance, target_external_reference, target_description_code
  );
  return resulting_balance;
end;
$$;

create or replace function public.start_billing_trial(
  target_organization_id uuid,
  target_plan_code text
)
returns public.organization_subscriptions
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller_id uuid := (select auth.uid());
  caller_role text;
  selected_plan public.billing_plans;
  selected_account public.credit_accounts;
  selected_subscription public.organization_subscriptions;
  seats integer;
  trial_microcredits bigint;
begin
  if caller_id is null then raise exception 'authentication required' using errcode = '42501'; end if;
  select member.role into caller_role
  from public.organization_members member
  where member.organization_id = target_organization_id and member.user_id = caller_id;
  if caller_role is null or caller_role not in ('owner', 'admin') then
    raise exception 'organization forbidden' using errcode = '42501';
  end if;
  select * into selected_plan from public.billing_plans plan
  where plan.code = target_plan_code and plan.active and plan.trial_days > 0;
  if not found then raise exception 'trial unavailable' using errcode = '22023'; end if;
  if target_plan_code not in ('team', 'business')
     or not exists (
       select 1 from public.organizations organization
       where organization.id = target_organization_id and organization.kind = 'team'
     ) then
    raise exception 'trial requires team workspace' using errcode = '22023';
  end if;
  insert into public.billing_trial_claims (actor_id, organization_id, plan_code)
  values (caller_id, target_organization_id, target_plan_code);
  seats := greatest(1, (select count(*)::integer from public.organization_members member
    where member.organization_id = target_organization_id));
  insert into public.organization_subscriptions (
    organization_id, plan_code, status, seat_quantity, billing_cycle,
    current_period_start, current_period_end, trial_ends_at, trial_started_at
  ) values (
    target_organization_id, target_plan_code, 'trialing', seats, 'annual',
    now(), now() + make_interval(days => selected_plan.trial_days),
    now() + make_interval(days => selected_plan.trial_days), now()
  ) returning * into selected_subscription;

  trial_microcredits := greatest(1, selected_plan.included_monthly_credits * seats * 100);
  select * into selected_account from public.credit_accounts account
  where account.organization_id = target_organization_id for update;
  update public.credit_accounts account set
    balance_microcredits = account.balance_microcredits + trial_microcredits,
    lifetime_granted_microcredits = account.lifetime_granted_microcredits + trial_microcredits,
    version = account.version + 1,
    updated_at = now()
  where account.organization_id = target_organization_id;
  insert into public.credit_ledger (
    organization_id, actor_id, entry_type, amount_microcredits,
    balance_after_microcredits, external_reference, description_code
  ) values (
    target_organization_id, caller_id, 'grant', trial_microcredits,
    selected_account.balance_microcredits + trial_microcredits,
    'trial:' || caller_id::text, 'credits.trial'
  );
  return selected_subscription;
exception
  when unique_violation then
    raise exception 'trial already claimed' using errcode = '23505';
end;
$$;

revoke all on function public.billing_reserve_ai_credits(uuid,uuid,uuid,uuid,text,integer,integer) from public, anon, authenticated;
revoke all on function public.billing_settle_ai_credits(uuid,integer,integer) from public, anon, authenticated;
revoke all on function public.billing_release_ai_credits(uuid,text) from public, anon, authenticated;
revoke all on function public.billing_apply_credit_grant(uuid,bigint,text,text,text) from public, anon, authenticated;
grant execute on function public.billing_reserve_ai_credits(uuid,uuid,uuid,uuid,text,integer,integer) to service_role;
grant execute on function public.billing_settle_ai_credits(uuid,integer,integer) to service_role;
grant execute on function public.billing_release_ai_credits(uuid,text) to service_role;
grant execute on function public.billing_apply_credit_grant(uuid,bigint,text,text,text) to service_role;
revoke all on function public.start_billing_trial(uuid,text) from public, anon;
grant execute on function public.start_billing_trial(uuid,text) to authenticated;

revoke all on function private.create_credit_account() from public, anon, authenticated;
revoke all on function private.reject_credit_ledger_mutation() from public, anon, authenticated;

alter table public.billing_plans enable row level security;
alter table public.organization_subscriptions enable row level security;
alter table public.billing_trial_claims enable row level security;
alter table public.credit_accounts enable row level security;
alter table public.model_price_versions enable row level security;
alter table public.ai_usage_requests enable row level security;
alter table public.credit_holds enable row level security;
alter table public.credit_ledger enable row level security;

create policy billing_plans_public_select on public.billing_plans for select to anon, authenticated
using (active);
create policy model_price_versions_authenticated_select on public.model_price_versions for select to authenticated
using (retired_at is null);
create policy organization_subscriptions_member_select on public.organization_subscriptions for select to authenticated
using ((select private.is_organization_member(organization_id)));
create policy billing_trial_claims_member_select on public.billing_trial_claims for select to authenticated
using ((select private.is_organization_member(organization_id)));
create policy credit_accounts_member_select on public.credit_accounts for select to authenticated
using ((select private.is_organization_member(organization_id)));
create policy ai_usage_requests_member_select on public.ai_usage_requests for select to authenticated
using ((select private.is_organization_member(organization_id)));
create policy credit_holds_member_select on public.credit_holds for select to authenticated
using ((select private.is_organization_member(organization_id)));
create policy credit_ledger_member_select on public.credit_ledger for select to authenticated
using ((select private.is_organization_member(organization_id)));

revoke all on table public.billing_plans, public.organization_subscriptions, public.billing_trial_claims, public.credit_accounts,
  public.model_price_versions, public.ai_usage_requests, public.credit_holds, public.credit_ledger
from anon, authenticated;
grant select on table public.billing_plans to anon, authenticated;
grant select on table public.organization_subscriptions, public.credit_accounts,
  public.model_price_versions, public.ai_usage_requests, public.credit_holds, public.credit_ledger
to authenticated;
grant usage, select on sequence public.credit_ledger_id_seq, public.model_price_versions_id_seq to service_role;
