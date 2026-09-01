import { describe, expect, it } from "vitest";
import type { CredentialStatus } from "../types/session";
import { vaultUnlockAction } from "./vault-unlock";

function status(overrides: Partial<CredentialStatus>): CredentialStatus {
  return {
    vaultInitialized: false,
    vaultUnlocked: false,
    hasCredential: false,
    platformUnlockSupported: true,
    platformUnlockAvailable: true,
    platformUnlockConfigured: false,
    ...overrides,
  };
}

describe("vaultUnlockAction", () => {
  it("creates new platform-backed vaults without a user password", () => {
    expect(vaultUnlockAction(status({}))).toBe("initialize-platform");
  });

  it("uses the platform key for a migrated locked vault", () => {
    expect(vaultUnlockAction(status({ vaultInitialized: true, platformUnlockConfigured: true }))).toBe("unlock-platform");
  });

  it("asks once for the password when an existing vault still needs migration", () => {
    expect(vaultUnlockAction(status({ vaultInitialized: true }))).toBe("unlock-password");
  });

  it("does nothing when the vault is already unlocked", () => {
    expect(vaultUnlockAction(status({ vaultInitialized: true, vaultUnlocked: true }))).toBe("none");
  });

  it("falls back to password initialization when the platform store is unavailable", () => {
    expect(vaultUnlockAction(status({ platformUnlockAvailable: false }))).toBe("unlock-password");
  });

  it("falls back to a password when a configured platform store becomes unavailable", () => {
    expect(vaultUnlockAction(status({ vaultInitialized: true, platformUnlockConfigured: true, platformUnlockAvailable: false }))).toBe("unlock-password");
  });
});
