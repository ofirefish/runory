-- Add Free plan: local workspace features without cloud sync or Managed AI.
alter table public.billing_plans drop constraint if exists billing_plans_code_check;
alter table public.billing_plans
  add constraint billing_plans_code_check check (code in ('free', 'pro', 'team', 'business'));

insert into public.billing_plans (
  code, monthly_price_cents, annual_monthly_price_cents, currency, per_seat,
  trial_days, included_monthly_credits, feature_codes, sort_order
) values (
  'free', 0, 0, 'USD', false, 0, 0,
  '["local-ssh","personal-vault","files-sftp","multi-session","local-ops"]'::jsonb,
  0
)
on conflict (code) do update set
  monthly_price_cents = excluded.monthly_price_cents,
  annual_monthly_price_cents = excluded.annual_monthly_price_cents,
  per_seat = excluded.per_seat,
  trial_days = excluded.trial_days,
  included_monthly_credits = excluded.included_monthly_credits,
  feature_codes = excluded.feature_codes,
  sort_order = excluded.sort_order,
  active = true,
  updated_at = now();
