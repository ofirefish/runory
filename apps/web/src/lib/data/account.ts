import "server-only";
import { createSupabaseServerClient } from "@/lib/supabase/server";

export type AccountDashboard = {
  userId: string;
  email: string;
  displayName: string;
  workspaces: Array<{ id: string; name: string; kind: "personal" | "team"; revision: number | null; updatedAt: string | null }>;
};

export type AccountResult =
  | { status: "configuration" }
  | { status: "signedOut" }
  | { status: "ok"; account: AccountDashboard };

export async function loadAccountDashboard(): Promise<AccountResult> {
  const supabase = await createSupabaseServerClient();
  if (!supabase) return { status: "configuration" };
  const { data: { user } } = await supabase.auth.getUser();
  if (!user) return { status: "signedOut" };

  let { data: profile } = await supabase.from("user_profiles").select("display_name").eq("id", user.id).maybeSingle();
  if (!profile) {
    const fallbackName = user.email?.split("@")[0]?.slice(0, 64) || "Runory";
    const { data } = await supabase.rpc("ensure_my_profile", { target_display_name: fallbackName });
    profile = data;
  }

  let { data: organizations } = await supabase.from("organizations").select("id,name,kind").order("created_at");
  if (!organizations?.length && user.email_confirmed_at) {
    await supabase.rpc("ensure_personal_workspace");
    const refreshed = await supabase.from("organizations").select("id,name,kind").order("created_at");
    organizations = refreshed.data;
  }

  const organizationIds = (organizations ?? []).map((item) => item.id);
  const syncRows = organizationIds.length
    ? (await supabase.from("sync_objects").select("organization_id,revision,updated_at").eq("kind", "inventory").in("organization_id", organizationIds)).data ?? []
    : [];
  const syncByOrganization = new Map(syncRows.map((row) => [row.organization_id, row]));

  return {
    status: "ok",
    account: {
      userId: user.id,
      email: user.email ?? "",
      displayName: profile?.display_name ?? user.email?.split("@")[0] ?? "Runory",
      workspaces: (organizations ?? []).map((organization) => {
        const sync = syncByOrganization.get(organization.id);
        return { id: organization.id, name: organization.name, kind: organization.kind, revision: sync?.revision ?? null, updatedAt: sync?.updated_at ?? null };
      }),
    },
  };
}
