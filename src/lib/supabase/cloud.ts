import type { AuthChangeEvent, Session, User } from "@supabase/supabase-js";
import type { AccessPolicy, AccessPolicyAction, AccessPolicyEffect, CloudAuditRecord, CloudEncryptedPayload, MyOrganizationInvite, Organization, OrganizationInvite, OrganizationMember, OrganizationMemberDetails, OrganizationRole, SyncObject } from "../../types/cloud";
import { supabase } from "./client";

const client = () => {
  if (!supabase) throw new Error("CLOUD_NOT_CONFIGURED");
  return supabase;
};

export const cloudSession = async (): Promise<Session | null> => (await client().auth.getSession()).data.session;
export const onCloudAuthStateChange = (callback: (event: AuthChangeEvent, session: Session | null) => void) =>
  client().auth.onAuthStateChange(callback).data.subscription;
export const cloudSignUp = async (email: string, password: string): Promise<User | null> => {
  const { data, error } = await client().auth.signUp({ email, password });
  if (error) throw error;
  return data.user;
};
export const cloudSignIn = async (email: string, password: string): Promise<Session> => {
  const { data, error } = await client().auth.signInWithPassword({ email, password });
  if (error || !data.session) throw error ?? new Error("AUTH_FAILED");
  return data.session;
};
export const cloudSignOut = async () => { const { error } = await client().auth.signOut(); if (error) throw error; };
export const listOrganizations = async (): Promise<Organization[]> => {
  const { data, error } = await client().from("organizations").select("id,name,owner_id,created_at,updated_at").order("created_at");
  if (error) throw error;
  return data;
};
export const createOrganization = async (name: string, ownerId: string): Promise<Organization> => {
  const { data, error } = await client().from("organizations").insert({ name, owner_id: ownerId }).select("id,name,owner_id,created_at,updated_at").single();
  if (error) throw error;
  return data;
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

export const writeEncryptedInventory = async (organizationId: string, encryptedPayload: CloudEncryptedPayload): Promise<SyncObject> => {
  const current = await loadEncryptedInventory(organizationId);
  const { data, error } = await client().rpc("write_sync_object", {
    target_organization_id: organizationId,
    target_kind: "inventory",
    target_logical_id: organizationId,
    target_encrypted_payload: encryptedPayload,
    expected_revision: current?.revision ?? 0,
  });
  if (error) throw error;
  return data;
};
