import type { CredentialStatus } from "../types/session";

export type VaultUnlockAction = "none" | "initialize-platform" | "unlock-platform" | "unlock-password";

export function vaultUnlockAction(status: CredentialStatus | null): VaultUnlockAction {
  if (status?.vaultUnlocked) return "none";
  if (!status?.vaultInitialized) {
    return status?.platformUnlockAvailable ? "initialize-platform" : "unlock-password";
  }
  return status.platformUnlockConfigured && status.platformUnlockAvailable ? "unlock-platform" : "unlock-password";
}
