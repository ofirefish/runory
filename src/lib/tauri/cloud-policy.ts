import { invoke } from "@tauri-apps/api/core";

export type CloudPolicyBindingRequest = {
  organizationId: string;
  profileIds: string[];
  supabaseUrl: string;
  publishableKey: string;
  accessToken: string;
  expiresAt: number;
};

export type CloudPolicyCredentialRequest = Omit<CloudPolicyBindingRequest, "organizationId" | "profileIds">;
export type CloudPolicyStatus = {
  organizationId: string;
  enabled: boolean;
  authenticated: boolean;
  profileCount: number;
};

export const bindCloudPolicy = (request: CloudPolicyBindingRequest) =>
  invoke<CloudPolicyStatus>("cloud_policy_bind", { request });

export const refreshCloudPolicy = (request: CloudPolicyCredentialRequest) =>
  invoke<void>("cloud_policy_refresh", { request });
export const lockCloudPolicy = () => invoke<void>("cloud_policy_lock");
export const unbindCloudPolicy = (organizationId: string) =>
  invoke<CloudPolicyStatus>("cloud_policy_unbind", { request: { organizationId } });
export const cloudPolicyStatus = (organizationId: string) =>
  invoke<CloudPolicyStatus>("cloud_policy_status", { request: { organizationId } });
