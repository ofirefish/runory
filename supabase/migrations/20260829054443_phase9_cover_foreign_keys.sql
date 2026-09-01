-- Postgres does not automatically index the referencing side of foreign keys.
-- These indexes keep user deletion/restriction checks and actor joins bounded as data grows.
create index organizations_owner_id_idx on public.organizations (owner_id);
create index organization_invites_invited_by_idx on public.organization_invites (invited_by);
create index sync_objects_updated_by_idx on public.sync_objects (updated_by);
create index access_policies_created_by_idx on public.access_policies (created_by);
