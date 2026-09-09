import "server-only";
import type { User } from "@supabase/supabase-js";
import { createSupabaseAdminClient } from "@/lib/supabase/admin";
import { createSupabaseServerClient } from "@/lib/supabase/server";

type AdminContext =
  | { status: "configuration" | "signedOut" | "forbidden" | "unavailable" }
  | { status: "ok"; actorId: string; role: "owner" | "support_viewer"; admin: NonNullable<ReturnType<typeof createSupabaseAdminClient>> };

export type AdminAccountRow = {
  id: string;
  email: string;
  displayName: string;
  confirmed: boolean;
  createdAt: string;
  lastSignInAt: string | null;
  organizationCount: number;
  lastSyncAt: string | null;
};

export type AdminAccountDetail = AdminAccountRow & {
  memberships: Array<{ organizationId: string; organizationName: string; organizationKind: "personal" | "team"; role: string }>;
  syncObjectCount: number;
};

export async function getAdminContext(): Promise<AdminContext> {
  const [supabase, admin] = await Promise.all([createSupabaseServerClient(), Promise.resolve(createSupabaseAdminClient())]);
  if (!supabase) return { status: "configuration" };
  const { data: { user } } = await supabase.auth.getUser();
  if (!user) return { status: "signedOut" };
  if (!admin) return { status: "configuration" };
  const { data, error } = await admin.from("platform_admins").select("role,enabled").eq("user_id", user.id).maybeSingle();
  if (error) return { status: "unavailable" };
  if (!data?.enabled || (data.role !== "owner" && data.role !== "support_viewer")) return { status: "forbidden" };
  return { status: "ok", actorId: user.id, role: data.role, admin };
}

function newestDate(values: Array<string | null | undefined>): string | null {
  return values.filter((value): value is string => Boolean(value)).sort().at(-1) ?? null;
}

async function enrichUsers(admin: NonNullable<ReturnType<typeof createSupabaseAdminClient>>, users: User[]): Promise<AdminAccountRow[]> {
  if (!users.length) return [];
  const ids = users.map((user) => user.id);
  const [{ data: profiles }, { data: memberships }] = await Promise.all([
    admin.from("user_profiles").select("id,display_name").in("id", ids),
    admin.from("organization_members").select("user_id,organization_id").in("user_id", ids),
  ]);
  const profileById = new Map((profiles ?? []).map((profile) => [profile.id, profile.display_name]));
  const membershipRows = memberships ?? [];
  const organizationIds = [...new Set(membershipRows.map((membership) => membership.organization_id))];
  const syncRows = organizationIds.length
    ? (await admin.from("sync_objects").select("organization_id,updated_at").eq("kind", "inventory").in("organization_id", organizationIds)).data ?? []
    : [];

  return users.map((user) => {
    const userOrganizationIds = membershipRows.filter((membership) => membership.user_id === user.id).map((membership) => membership.organization_id);
    return {
      id: user.id,
      email: user.email ?? "",
      displayName: profileById.get(user.id) ?? user.email?.split("@")[0] ?? "—",
      confirmed: Boolean(user.email_confirmed_at),
      createdAt: user.created_at,
      lastSignInAt: user.last_sign_in_at ?? null,
      organizationCount: userOrganizationIds.length,
      lastSyncAt: newestDate(syncRows.filter((row) => userOrganizationIds.includes(row.organization_id)).map((row) => row.updated_at)),
    };
  });
}

export async function listAdminAccounts(page = 1) {
  const context = await getAdminContext();
  if (context.status !== "ok") return { status: context.status } as const;
  const { data, error } = await context.admin.auth.admin.listUsers({ page, perPage: 50 });
  if (error) return { status: "unavailable" } as const;
  await context.admin.from("platform_admin_audit").insert({ actor_id: context.actorId, action: "accounts.list" });
  return { status: "ok", accounts: await enrichUsers(context.admin, data.users), page, hasNext: data.users.length === 50 } as const;
}

export async function getAdminAccount(userId: string) {
  const context = await getAdminContext();
  if (context.status !== "ok") return { status: context.status } as const;
  const { data: userData, error } = await context.admin.auth.admin.getUserById(userId);
  if (error || !userData.user) return { status: "missing" } as const;
  const [row] = await enrichUsers(context.admin, [userData.user]);
  const { data: memberships } = await context.admin.from("organization_members").select("organization_id,role").eq("user_id", userId);
  const organizationIds = (memberships ?? []).map((membership) => membership.organization_id);
  const [{ data: organizations }, { count: syncObjectCount }] = await Promise.all([
    organizationIds.length ? context.admin.from("organizations").select("id,name,kind").in("id", organizationIds) : Promise.resolve({ data: [] }),
    organizationIds.length ? context.admin.from("sync_objects").select("id", { count: "exact", head: true }).in("organization_id", organizationIds) : Promise.resolve({ count: 0 }),
  ]);
  const organizationById = new Map((organizations ?? []).map((organization) => [organization.id, organization]));
  await context.admin.from("platform_admin_audit").insert({ actor_id: context.actorId, action: "accounts.read", target_user_id: userId });
  const detail: AdminAccountDetail = {
    ...row,
    memberships: (memberships ?? []).map((membership) => {
      const organization = organizationById.get(membership.organization_id);
      return { organizationId: membership.organization_id, organizationName: organization?.name ?? "—", organizationKind: organization?.kind ?? "team", role: membership.role };
    }),
    syncObjectCount: syncObjectCount ?? 0,
  };
  return { status: "ok", account: detail } as const;
}
