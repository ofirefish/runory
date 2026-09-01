export type OrganizationRole = "owner" | "admin" | "operator" | "viewer";
export type Organization = { id: string; name: string; owner_id: string; created_at: string; updated_at: string };
export type OrganizationMember = { organization_id: string; user_id: string; role: OrganizationRole; created_at: string };
export type OrganizationMemberDetails = { user_id: string; email: string; role: OrganizationRole; created_at: string };
export type OrganizationInvite = { id: string; organization_id: string; email: string; role: Exclude<OrganizationRole, "owner">; invited_by: string; expires_at: string; accepted_at: string | null; created_at: string };
export type MyOrganizationInvite = Pick<OrganizationInvite, "id" | "organization_id" | "email" | "role" | "expires_at" | "created_at"> & { organization_name: string };
export type AccessPolicyEffect = "allow" | "deny";
export type AccessPolicyAction = "connect" | "read-files" | "write-files" | "operate" | "deploy" | "ai-execute";
export type AccessPolicy = {
  id: string; organization_id: string; name: string; effect: AccessPolicyEffect;
  action: AccessPolicyAction; resource_selector: Json; created_by: string;
  created_at: string; updated_at: string;
};
export type AuditResult = "started" | "succeeded" | "failed" | "denied";
export type CloudAuditRecord = {
  id: number; organization_id: string; actor_id: string; action: string;
  resource_type: string; resource_id: string | null; result: AuditResult;
  error_code: string | null; occurred_at: string;
};
export type Json = string | number | boolean | null | { [key: string]: Json | undefined } | Json[];
export type CloudEncryptedPayload = { version: number; salt: number[]; nonce: number[]; ciphertext: number[] };
export type SyncObject = {
  id: string; organization_id: string; kind: "profile" | "group" | "inventory" | "known-host";
  logical_id: string; encrypted_payload: Json; revision: number; updated_by: string;
  created_at: string; updated_at: string;
};
export type CloudImportPreview = {
  importId: string; groupAdditions: number; groupUpdates: number; profileAdditions: number;
  profileUpdates: number; groupDeletions: number; profileDeletions: number;
  localNewer: number; conflicts: number; conflictItems: CloudConflictItem[];
};
export type CloudObjectKind = "group" | "profile";
export type CloudConflictItem = { kind: CloudObjectKind; id: string; label: string; localUpdatedAt: string; remoteUpdatedAt: string; remoteDeleted: boolean };
export type CloudConflictDecision = CloudConflictItem & { resolution: "keepLocal" | "useRemote" };
export type CloudApplyResult = { groupsApplied: number; profilesApplied: number; groupsDeleted: number; profilesDeleted: number; skipped: number };

export type CloudDatabase = {
  public: {
    Tables: {
      organizations: {
        Row: Organization;
        Insert: { id?: string; name: string; owner_id: string; created_at?: string; updated_at?: string };
        Update: { name?: string; updated_at?: string };
        Relationships: [];
      };
      organization_members: {
        Row: OrganizationMember;
        Insert: { organization_id: string; user_id: string; role: OrganizationRole; created_at?: string };
        Update: { role?: OrganizationRole };
        Relationships: [];
      };
      organization_invites: {
        Row: OrganizationInvite;
        Insert: Omit<OrganizationInvite, "id" | "accepted_at" | "created_at"> & { id?: string; accepted_at?: string | null; created_at?: string };
        Update: Partial<Pick<OrganizationInvite, "role" | "expires_at" | "accepted_at">>;
        Relationships: [];
      };
      sync_objects: {
        Row: SyncObject;
        Insert: Omit<SyncObject, "id" | "created_at" | "updated_at" | "revision"> & { id?: string; revision?: number; created_at?: string; updated_at?: string };
        Update: Partial<Omit<SyncObject, "id" | "organization_id" | "kind" | "logical_id" | "created_at">>;
        Relationships: [];
      };
      access_policies: {
        Row: AccessPolicy;
        Insert: Omit<AccessPolicy, "id" | "created_at" | "updated_at"> & { id?: string; created_at?: string; updated_at?: string };
        Update: Partial<Pick<AccessPolicy, "name" | "effect" | "action" | "resource_selector" | "updated_at">>;
        Relationships: [];
      };
      audit_records: {
        Row: CloudAuditRecord;
        Insert: Omit<CloudAuditRecord, "id" | "occurred_at"> & { occurred_at?: string };
        Update: never;
        Relationships: [];
      };
    };
    Views: Record<string, never>;
    Functions: {
      write_sync_object: {
        Args: { target_organization_id: string; target_kind: string; target_logical_id: string; target_encrypted_payload: Json; expected_revision: number };
        Returns: SyncObject;
      };
      accept_organization_invite: { Args: { target_invite_id: string }; Returns: string };
      create_organization_invite: { Args: { target_organization_id: string; target_email: string; target_role: string }; Returns: OrganizationInvite };
      list_my_organization_invites: { Args: Record<PropertyKey, never>; Returns: MyOrganizationInvite[] };
      list_organization_members: { Args: { target_organization_id: string }; Returns: OrganizationMemberDetails[] };
      revoke_organization_invite: { Args: { target_invite_id: string }; Returns: undefined };
      update_organization_member_role: { Args: { target_organization_id: string; target_user_id: string; target_role: string }; Returns: OrganizationMember };
      remove_organization_member: { Args: { target_organization_id: string; target_user_id: string }; Returns: undefined };
      create_access_policy: { Args: { target_organization_id: string; target_name: string; target_effect: string; target_action: string; target_resource_selector: Json }; Returns: AccessPolicy };
      delete_access_policy: { Args: { target_policy_id: string }; Returns: undefined };
      list_audit_records: { Args: { target_organization_id: string; cursor_occurred_at: string | null; cursor_id: number | null; target_page_size: number }; Returns: CloudAuditRecord[] };
      evaluate_access_policy: { Args: { target_organization_id: string; target_action: AccessPolicyAction; target_resource_type: "server-profile"; target_resource_id: string }; Returns: boolean };
    };
    Enums: Record<string, never>;
    CompositeTypes: Record<string, never>;
  };
};
