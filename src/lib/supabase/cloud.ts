import type { AuthChangeEvent, Session, SupabaseClient, User } from "@supabase/supabase-js";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { AccessPolicy, AccessPolicyAction, AccessPolicyEffect, BillingPlan, BillingPlanCode, CloudAuditRecord, CloudDatabase, CloudEncryptedPayload, CloudUserProfile, CreditAccount, CreditLedgerEntry, MyOrganizationInvite, Organization, OrganizationInvite, OrganizationMember, OrganizationMemberDetails, OrganizationRole, OrganizationSubscription, SyncObject } from "../../types/cloud";
import { cloudEndpoint, createCloudOAuthClient, supabase } from "./client";
import { authDeepLinkDebugInfo, authErrorDebugInfo, logAuthDebug } from "./auth-debug";
import { clearPersistedCloudSession, persistCloudSession } from "./session-persistence";

const authWebBaseUrl = "https://runory.app";
const cloudOAuthRedirectUrl = "runory://auth/callback";
export const cloudOAuthErrorEvent = "runory:cloud-oauth-error";
export const cloudProfileUpdatedEvent = "runory:cloud-profile-updated";
let pendingOAuthClient: SupabaseClient<CloudDatabase> | null = null;

const client = () => {
  if (!supabase) throw new Error("CLOUD_NOT_CONFIGURED");
  return supabase;
};

export const cloudSession = async (): Promise<Session | null> => (await client().auth.getSession()).data.session;
export const onCloudAuthStateChange = (callback: (event: AuthChangeEvent, session: Session | null) => void) =>
  client().auth.onAuthStateChange(callback).data.subscription;
export const cloudSignUp = async (email: string, password: string): Promise<User | null> => {
  const redirectTo = `${authWebBaseUrl}/auth/confirm`;
  logAuthDebug("signUp:start", {
    email,
    redirectTo,
    cloudEndpoint: cloudEndpoint ?? null,
    cloudConfigured: Boolean(supabase),
  });
  const { data, error } = await client().auth.signUp({
    email,
    password,
    options: { emailRedirectTo: redirectTo },
  });
  if (error) {
    logAuthDebug("signUp:error", authErrorDebugInfo(error));
    throw error;
  }
  logAuthDebug("signUp:ok", {
    userId: data.user?.id ?? null,
    email: data.user?.email ?? email,
    emailConfirmedAt: data.user?.email_confirmed_at ?? null,
    identities: data.user?.identities?.length ?? 0,
    hasSession: Boolean(data.session),
  });
  return data.user;
};
export const cloudSignIn = async (email: string, password: string): Promise<Session> => {
  const { data, error } = await client().auth.signInWithPassword({ email, password });
  if (error || !data.session) throw error ?? new Error("AUTH_FAILED");
  await persistCloudSession(data.session);
  return data.session;
};
type CloudOAuthProvider = "github" | "google";

const cloudSignInWithProvider = async (provider: CloudOAuthProvider): Promise<void> => {
  const oauth = createCloudOAuthClient();
  pendingOAuthClient = oauth;
  const { data, error } = await oauth.auth.signInWithOAuth({
    provider,
    options: { redirectTo: cloudOAuthRedirectUrl, skipBrowserRedirect: true },
  });
  if (error || !data.url || !cloudEndpoint) {
    pendingOAuthClient = null;
    throw error ?? new Error("OAUTH_URL_MISSING");
  }
  const authorizeUrl = new URL(data.url);
  const configured = new URL(cloudEndpoint);
  if (authorizeUrl.origin !== configured.origin
    || authorizeUrl.pathname !== `${configured.pathname.replace(/\/$/, "")}/auth/v1/authorize`
    || authorizeUrl.searchParams.get("provider") !== provider) {
    pendingOAuthClient = null;
    throw new Error("OAUTH_URL_INVALID");
  }
  await openUrl(authorizeUrl.toString());
};

export const cloudSignInWithGoogle = (): Promise<void> => cloudSignInWithProvider("google");
export const cloudSignInWithGitHub = (): Promise<void> => cloudSignInWithProvider("github");

export const isCloudOAuthRedirect = (value: string): boolean => {
  try {
    const url = new URL(value);
    return url.protocol === "runory:" && url.hostname === "auth" && url.pathname === "/callback";
  } catch {
    return false;
  }
};

/** Tokens must stay in the fragment so they are not logged as query parameters. */
export function buildCloudEmailConfirmDeepLink(accessToken: string, refreshToken: string): string {
  const hash = new URLSearchParams({
    access_token: accessToken,
    refresh_token: refreshToken,
    type: "email_confirm",
  });
  return `${cloudOAuthRedirectUrl}#${hash.toString()}`;
}

export const completeCloudOAuthRedirect = async (value: string): Promise<Session> => {
  if (!isCloudOAuthRedirect(value)) throw new Error("OAUTH_REDIRECT_INVALID");
  const oauth = pendingOAuthClient;
  if (!oauth) throw new Error("OAUTH_ATTEMPT_MISSING");
  const url = new URL(value);
  const fragment = new URLSearchParams(url.hash.startsWith("#") ? url.hash.slice(1) : url.hash);
  if (url.searchParams.has("error") || fragment.has("error")) {
    pendingOAuthClient = null;
    throw new Error("OAUTH_PROVIDER_REJECTED");
  }
  const code = url.searchParams.get("code");
  if (!code) {
    pendingOAuthClient = null;
    throw new Error("OAUTH_CODE_MISSING");
  }
  const { data, error } = await oauth.auth.exchangeCodeForSession(code);
  if (error || !data.session) throw error ?? new Error("OAUTH_SESSION_MISSING");
  const { data: applied, error: applyError } = await client().auth.setSession({
    access_token: data.session.access_token,
    refresh_token: data.session.refresh_token,
  });
  if (applyError || !applied.session) throw applyError ?? new Error("OAUTH_SESSION_MISSING");
  await persistCloudSession(applied.session);
  // Dropping the one-attempt client also drops its in-memory PKCE verifier and session.
  pendingOAuthClient = null;
  return applied.session;
};

/**
 * Handles both OAuth PKCE (`?code=` with a pending attempt) and email-confirm
 * handoff (`#access_token` + `#refresh_token` from runory.app after HTTPS confirm).
 */
export const completeCloudAuthDeepLink = async (value: string): Promise<Session> => {
  logAuthDebug("deepLink:start", authDeepLinkDebugInfo(value));
  if (!isCloudOAuthRedirect(value)) throw new Error("OAUTH_REDIRECT_INVALID");
  const url = new URL(value);
  const fragment = new URLSearchParams(url.hash.startsWith("#") ? url.hash.slice(1) : url.hash);
  if (url.searchParams.has("error") || fragment.has("error")) {
    pendingOAuthClient = null;
    throw new Error("OAUTH_PROVIDER_REJECTED");
  }
  if (url.searchParams.has("access_token") || url.searchParams.has("refresh_token")) {
    throw new Error("AUTH_DEEPLINK_TOKEN_IN_QUERY");
  }

  const code = url.searchParams.get("code");
  if (pendingOAuthClient && code) {
    logAuthDebug("deepLink:oauth-pkce");
    return completeCloudOAuthRedirect(value);
  }

  const accessToken = fragment.get("access_token");
  const refreshToken = fragment.get("refresh_token");
  if (accessToken && refreshToken) {
    logAuthDebug("deepLink:email-confirm-setSession", { type: fragment.get("type") });
    const { data, error } = await client().auth.setSession({
      access_token: accessToken,
      refresh_token: refreshToken,
    });
    if (error || !data.session) {
      if (error) logAuthDebug("deepLink:setSession-error", authErrorDebugInfo(error));
      throw error ?? new Error("AUTH_DEEPLINK_SESSION_MISSING");
    }
    await persistCloudSession(data.session);
    logAuthDebug("deepLink:email-confirm-ok", {
      userId: data.session.user.id,
      email: data.session.user.email ?? null,
      emailConfirmedAt: data.session.user.email_confirmed_at ?? null,
    });
    return data.session;
  }

  if (code) throw new Error("OAUTH_ATTEMPT_MISSING");
  throw new Error("AUTH_DEEPLINK_CREDENTIALS_MISSING");
};
export const cloudSignOut = async () => {
  const { error } = await client().auth.signOut();
  await clearPersistedCloudSession();
  if (error) throw error;
};
export const requestCloudPasswordReset = async (email: string): Promise<void> => {
  const { error } = await client().auth.resetPasswordForEmail(email, {
    redirectTo: `${authWebBaseUrl}/auth/reset`,
  });
  if (error) throw error;
};
export const consumeCloudAuthRedirect = async (): Promise<Session> => {
  const url = new URL(window.location.href);
  const fragment = new URLSearchParams(url.hash.startsWith("#") ? url.hash.slice(1) : url.hash);
  if (url.searchParams.has("error") || fragment.has("error")) throw new Error("AUTH_REDIRECT_FAILED");

  const code = url.searchParams.get("code");
  let session: Session | null = null;
  if (code) {
    const { data, error } = await client().auth.exchangeCodeForSession(code);
    if (error) throw error;
    session = data.session;
  } else {
    const accessToken = fragment.get("access_token");
    const refreshToken = fragment.get("refresh_token");
    if (accessToken && refreshToken) {
      const { data, error } = await client().auth.setSession({ access_token: accessToken, refresh_token: refreshToken });
      if (error) throw error;
      session = data.session;
    }
  }

  session ??= await cloudSession();
  if (!session) throw new Error("AUTH_REDIRECT_SESSION_MISSING");
  window.history.replaceState({}, document.title, url.pathname);
  return session;
};

export const updateCloudPassword = async (password: string): Promise<void> => {
  const { error } = await client().auth.updateUser({ password });
  if (error) throw error;
};
export const listOrganizations = async (): Promise<Organization[]> => {
  const { data, error } = await client().from("organizations").select("id,name,kind,owner_id,created_at,updated_at").order("created_at");
  if (error) throw error;
  return data;
};
export const createOrganization = async (name: string, ownerId: string): Promise<Organization> => {
  const id = crypto.randomUUID();
  const { error: insertError } = await client().from("organizations")
    .insert({ id, name, kind: "team", owner_id: ownerId });
  if (insertError) throw insertError;

  // The AFTER INSERT trigger establishes the owner's membership. Query only
  // after that statement completes so the organization SELECT policy can see it.
  const { data, error } = await client().from("organizations")
    .select("id,name,kind,owner_id,created_at,updated_at").eq("id", id).single();
  if (error) throw error;
  return data;
};

export const listBillingPlans = async (): Promise<BillingPlan[]> => {
  const { data, error } = await client().from("billing_plans")
    .select("code,monthly_price_cents,annual_monthly_price_cents,currency,per_seat,trial_days,included_monthly_credits,feature_codes,sort_order,active,created_at,updated_at")
    .eq("active", true).order("sort_order");
  if (error) throw error;
  return data;
};

export const getOrganizationSubscription = async (organizationId: string): Promise<OrganizationSubscription | null> => {
  const { data, error } = await client().from("organization_subscriptions")
    .select("organization_id,plan_code,status,seat_quantity,billing_cycle,current_period_start,current_period_end,trial_ends_at,trial_started_at,cancel_at_period_end,provider_customer_ref,provider_subscription_ref,created_at,updated_at")
    .eq("organization_id", organizationId).maybeSingle();
  if (error) throw error;
  return data;
};

export const getCreditAccount = async (organizationId: string): Promise<CreditAccount | null> => {
  const { data, error } = await client().from("credit_accounts")
    .select("organization_id,balance_microcredits,held_microcredits,lifetime_granted_microcredits,lifetime_spent_microcredits,version,updated_at")
    .eq("organization_id", organizationId).maybeSingle();
  if (error) throw error;
  return data;
};

export const listCreditLedger = async (organizationId: string, limit = 20): Promise<CreditLedgerEntry[]> => {
  const { data, error } = await client().from("credit_ledger")
    .select("id,organization_id,actor_id,request_id,entry_type,amount_microcredits,balance_after_microcredits,external_reference,description_code,created_at")
    .eq("organization_id", organizationId).order("created_at", { ascending: false }).limit(limit);
  if (error) throw error;
  return data;
};

export const startBillingTrial = async (organizationId: string, planCode: BillingPlanCode): Promise<OrganizationSubscription> => {
  const { data, error } = await client().rpc("start_billing_trial", {
    target_organization_id: organizationId,
    target_plan_code: planCode,
  });
  if (error) throw error;
  return data;
};

export const ensurePersonalWorkspace = async (): Promise<Organization> => {
  const { data, error } = await client().rpc("ensure_personal_workspace");
  if (error) throw error;
  return data;
};

export const ensureMyCloudProfile = async (displayName: string): Promise<CloudUserProfile> => {
  const { data, error } = await client().rpc("ensure_my_profile", { target_display_name: displayName });
  if (error) throw error;
  return data;
};

export const loadMyCloudProfile = async (userId: string): Promise<CloudUserProfile | null> => {
  const { data, error } = await client().from("user_profiles")
    .select("id,display_name,avatar_path,avatar_version,created_at,updated_at")
    .eq("id", userId).maybeSingle();
  if (error) throw error;
  return data;
};

export const updateMyCloudDisplayName = async (displayName: string): Promise<CloudUserProfile> => {
  const { data, error } = await client().rpc("update_my_display_name", { target_display_name: displayName });
  if (error) throw error;
  return data;
};

export const uploadCloudAvatar = async (userId: string, avatar: Blob): Promise<string> => {
  const path = `${userId}/${crypto.randomUUID()}.webp`;
  const { error } = await client().storage.from("avatars").upload(path, avatar, {
    cacheControl: "3600",
    contentType: "image/webp",
    upsert: false,
  });
  if (error) throw error;
  return path;
};

export const setMyCloudAvatar = async (avatarPath: string | null, expectedVersion: number): Promise<CloudUserProfile> => {
  const { data, error } = await client().rpc("set_my_avatar", {
    target_avatar_path: avatarPath,
    expected_avatar_version: expectedVersion,
  });
  if (error) throw error;
  return data;
};

export const deleteCloudAvatar = async (path: string): Promise<void> => {
  const { error } = await client().storage.from("avatars").remove([path]);
  if (error) throw error;
};

export const createCloudAvatarUrl = async (path: string): Promise<string> => {
  const { data, error } = await client().storage.from("avatars").createSignedUrl(path, 3600);
  if (error) throw error;
  return data.signedUrl;
};

export const getMyMembership = async (organizationId: string, userId: string): Promise<OrganizationMember | null> => {
  const { data, error } = await client().from("organization_members")
    .select("organization_id,user_id,role,created_at").eq("organization_id", organizationId).eq("user_id", userId).maybeSingle();
  if (error) throw error;
  return data;
};
export const listOrganizationMembers = async (organizationId: string): Promise<OrganizationMemberDetails[]> => {
  const { data, error } = await client().rpc("list_organization_members", { target_organization_id: organizationId });
  if (error) throw error;
  return data;
};
export const listOrganizationInvites = async (organizationId: string): Promise<OrganizationInvite[]> => {
  const { data, error } = await client().from("organization_invites")
    .select("id,organization_id,email,role,invited_by,expires_at,accepted_at,created_at")
    .eq("organization_id", organizationId).is("accepted_at", null).order("created_at", { ascending: false });
  if (error) throw error;
  return data;
};
export const createOrganizationInvite = async (organizationId: string, email: string, role: Exclude<OrganizationRole, "owner">): Promise<OrganizationInvite> => {
  const { data, error } = await client().rpc("create_organization_invite", {
    target_organization_id: organizationId, target_email: email, target_role: role,
  });
  if (error) throw error;
  return data;
};
export const revokeOrganizationInvite = async (id: string): Promise<void> => {
  const { error } = await client().rpc("revoke_organization_invite", { target_invite_id: id });
  if (error) throw error;
};
export const listMyOrganizationInvites = async (): Promise<MyOrganizationInvite[]> => {
  const { data, error } = await client().rpc("list_my_organization_invites");
  if (error) throw error;
  return data;
};
export const acceptOrganizationInvite = async (id: string): Promise<string> => {
  const { data, error } = await client().rpc("accept_organization_invite", { target_invite_id: id });
  if (error) throw error;
  return data;
};
export const updateOrganizationMemberRole = async (organizationId: string, userId: string, role: Exclude<OrganizationRole, "owner">): Promise<OrganizationMember> => {
  const { data, error } = await client().rpc("update_organization_member_role", {
    target_organization_id: organizationId, target_user_id: userId, target_role: role,
  });
  if (error) throw error;
  return data;
};
export const removeOrganizationMember = async (organizationId: string, userId: string): Promise<void> => {
  const { error } = await client().rpc("remove_organization_member", {
    target_organization_id: organizationId, target_user_id: userId,
  });
  if (error) throw error;
};
export const listAccessPolicies = async (organizationId: string): Promise<AccessPolicy[]> => {
  const { data, error } = await client().from("access_policies")
    .select("id,organization_id,name,effect,action,resource_selector,created_by,created_at,updated_at")
    .eq("organization_id", organizationId).order("created_at");
  if (error) throw error;
  return data;
};
export const createAccessPolicy = async (organizationId: string, name: string, effect: AccessPolicyEffect, action: AccessPolicyAction): Promise<AccessPolicy> => {
  const { data, error } = await client().rpc("create_access_policy", {
    target_organization_id: organizationId, target_name: name, target_effect: effect,
    target_action: action, target_resource_selector: {},
  });
  if (error) throw error;
  return data;
};
export const deleteAccessPolicy = async (id: string): Promise<void> => {
  const { error } = await client().rpc("delete_access_policy", { target_policy_id: id });
  if (error) throw error;
};
export const listCloudAuditRecords = async (organizationId: string, cursor: Pick<CloudAuditRecord, "occurred_at" | "id"> | null, pageSize = 50): Promise<CloudAuditRecord[]> => {
  const { data, error } = await client().rpc("list_audit_records", {
    target_organization_id: organizationId,
    cursor_occurred_at: cursor?.occurred_at ?? null,
    cursor_id: cursor?.id ?? null,
    target_page_size: pageSize,
  });
  if (error) throw error;
  return data;
};

export const loadEncryptedInventory = async (organizationId: string): Promise<SyncObject | null> => {
  const { data, error } = await client().from("sync_objects")
    .select("id,organization_id,kind,logical_id,encrypted_payload,revision,updated_by,created_at,updated_at")
    .eq("organization_id", organizationId).eq("kind", "inventory").eq("logical_id", organizationId).maybeSingle();
  if (error) throw error;
  return data;
};

export const writeEncryptedInventory = async (
  organizationId: string,
  encryptedPayload: CloudEncryptedPayload,
  expectedRevision: number,
): Promise<SyncObject> => {
  const { data, error } = await client().rpc("write_sync_object", {
    target_organization_id: organizationId,
    target_kind: "inventory",
    target_logical_id: organizationId,
    target_encrypted_payload: encryptedPayload,
    expected_revision: expectedRevision,
  });
  if (error) throw error;
  return data;
};
