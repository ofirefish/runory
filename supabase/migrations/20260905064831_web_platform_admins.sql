create table public.platform_admins (
  user_id uuid primary key references auth.users(id) on delete cascade,
  role text not null check (role in ('owner', 'support_viewer')),
  enabled boolean not null default true,
  created_by uuid not null references auth.users(id) on delete restrict,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create index platform_admins_created_by_idx on public.platform_admins (created_by);

create table public.platform_admin_audit (
  id bigint generated always as identity primary key,
  actor_id uuid not null references auth.users(id) on delete restrict,
  action text not null check (char_length(action) between 1 and 100),
  target_user_id uuid references auth.users(id) on delete set null,
  occurred_at timestamptz not null default now()
);

create index platform_admin_audit_actor_time_idx
on public.platform_admin_audit (actor_id, occurred_at desc, id desc);

create index platform_admin_audit_target_user_idx
on public.platform_admin_audit (target_user_id)
where target_user_id is not null;

alter table public.platform_admins enable row level security;
alter table public.platform_admin_audit enable row level security;

-- There are intentionally no client-facing policies. Platform administration is
-- authorized from a server-only client after checking this live registry.
revoke all on table public.platform_admins, public.platform_admin_audit from public, anon, authenticated, service_role;
revoke all on sequence public.platform_admin_audit_id_seq from public, anon, authenticated, service_role;

grant select on table public.platform_admins to service_role;
grant select, insert on table public.platform_admin_audit to service_role;
grant usage, select on sequence public.platform_admin_audit_id_seq to service_role;
