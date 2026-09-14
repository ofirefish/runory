// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import type { Session } from "@supabase/supabase-js";
import { toast } from "sonner";
import { cloudSession, cloudSignIn, cloudSignInWithGitHub, cloudSignInWithGoogle, cloudSignUp, createOrganization, listOrganizations, loadEncryptedInventory, onCloudAuthStateChange, writeEncryptedInventory } from "../../lib/supabase/cloud";
import { cloudSyncKeyStatus, discardCloudSync, exportCloudSync, previewCloudSync, rotateCloudRecoveryPassphrase } from "../../lib/tauri/cloud";
import { refreshCloudPolicy } from "../../lib/tauri/cloud-policy";
import { AccountSettings } from "./AccountSettings";
import { CloudPanel } from "./CloudPanel";

vi.mock("../../lib/supabase/client", () => ({
  cloudConfigured: true,
  cloudEndpoint: "https://fixture.supabase.co",
  cloudPublishableKey: "fixture-publishable-key",
}));
vi.mock("sonner", () => ({
  toast: { error: vi.fn(), success: vi.fn() },
}));
vi.mock("../../lib/supabase/cloud", () => ({
  cloudOAuthErrorEvent: "runory:cloud-oauth-error",
  cloudSession: vi.fn().mockResolvedValue(null),
  cloudSignIn: vi.fn(),
  cloudSignInWithGitHub: vi.fn().mockResolvedValue(undefined),
  cloudSignInWithGoogle: vi.fn().mockResolvedValue(undefined),
  cloudSignOut: vi.fn(),
  cloudSignUp: vi.fn(),
  createOrganization: vi.fn(),
  ensureMyCloudProfile: vi.fn(),
  ensurePersonalWorkspace: vi.fn(),
  listOrganizations: vi.fn().mockResolvedValue([]),
  loadEncryptedInventory: vi.fn(),
  onCloudAuthStateChange: vi.fn(() => ({ unsubscribe: vi.fn() })),
  requestCloudPasswordReset: vi.fn(),
  writeEncryptedInventory: vi.fn(),
}));
vi.mock("../../lib/tauri/cloud", () => ({
  applyCloudSync: vi.fn(), cloudSyncKeyStatus: vi.fn(), discardCloudSync: vi.fn(), exportCloudSync: vi.fn(), forgetCloudSyncKey: vi.fn(), previewCloudSync: vi.fn(), rotateCloudRecoveryPassphrase: vi.fn(),
}));
vi.mock("../../lib/tauri/cloud-policy", () => ({ lockCloudPolicy: vi.fn(), refreshCloudPolicy: vi.fn() }));
vi.mock("./CloudAccountPanel", () => ({ CloudAccountPanel: () => null }));
vi.mock("./CloudGovernancePanel", () => ({ CloudGovernancePanel: () => null }));
vi.mock("./CloudTeamPanel", () => ({ CloudTeamPanel: () => null }));

let root: Root;
let container: HTMLDivElement;

const button = (key: string) => Array.from(container.querySelectorAll("button"))
  .find((item) => item.textContent === i18n.t(key)) as HTMLButtonElement;

async function fill(labelKey: string, value: string) {
  const input = container.querySelector<HTMLInputElement>(`[aria-label="${i18n.t(labelKey)}"]`)!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

beforeEach(async () => {
  vi.clearAllMocks();
  vi.mocked(cloudSession).mockResolvedValue(null);
  vi.mocked(listOrganizations).mockResolvedValue([]);
  vi.mocked(cloudSyncKeyStatus).mockResolvedValue({ configured: false, persistedOnDevice: false, secureStorageAvailable: true });
  await i18n.changeLanguage("zh-CN");
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () => { root.render(<AccountSettings />); });
});

async function renderSignedIn() {
  const session = {
    access_token: "fixture-access-token",
    expires_at: 4_102_444_800,
    user: { id: "user-1", email: "person@example.test" },
  } as unknown as Session;
  vi.mocked(cloudSession).mockResolvedValue(session);
  vi.mocked(listOrganizations).mockResolvedValue([{
    id: "organization-1",
    name: "Personal",
    kind: "personal",
    owner_id: "user-1",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  }]);
  await act(async () => { root.unmount(); });
  root = createRoot(container);
  await act(async () => {
    root.render(<CloudPanel />);
    await Promise.resolve();
    await Promise.resolve();
  });
}

afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  vi.unstubAllGlobals();
});

describe("cloud authentication controls", () => {
  it.each(["cloud.signIn", "cloud.signUp"])("shows validation instead of silently ignoring %s", async (key) => {
    await act(async () => { button(key).click(); });
    expect(container.querySelector('[role="alert"]')?.textContent).toBe(i18n.t("cloud.authEmailRequired"));
    expect(cloudSignIn).not.toHaveBeenCalled();
    expect(cloudSignUp).not.toHaveBeenCalled();
  });

  it("validates account passwords before calling Supabase", async () => {
    await fill("cloud.email", "person@example.test");
    await fill("cloud.password", "short");
    await act(async () => { button("cloud.signUp").click(); });
    expect(container.querySelector('[role="alert"]')?.textContent).toBe(i18n.t("cloud.authPasswordTooShort"));
    expect(cloudSignUp).not.toHaveBeenCalled();
  });

  it.each([
    ["cloud.signIn", cloudSignIn],
    ["cloud.signUp", cloudSignUp],
  ] as const)("submits valid credentials through %s", async (key, action) => {
    vi.mocked(action).mockRejectedValueOnce(new Error("fixture response"));
    await fill("cloud.email", "person@example.test");
    await fill("cloud.password", "password-123");
    await act(async () => { button(key).click(); });
    expect(action).toHaveBeenCalledExactlyOnceWith("person@example.test", "password-123");
  });

  it("surfaces Auth rate limits instead of a generic network failure", async () => {
    vi.mocked(cloudSignUp).mockRejectedValueOnce({ status: 429, message: "email rate limit exceeded" });
    await fill("cloud.email", "person@example.test");
    await fill("cloud.password", "password-123");
    await act(async () => { button("cloud.signUp").click(); await Promise.resolve(); });
    expect(toast.error).toHaveBeenCalledWith(i18n.t("cloud.authRateLimited"), { id: "cloud-account-error" });
  });

  it("toasts when email confirmation signs the desktop session in", async () => {
    const session = {
      access_token: "fixture-access-token",
      expires_at: 4_102_444_800,
      user: { id: "user-1", email: "person@example.test" },
    } as unknown as Session;
    let emit: ((event: string, next: Session | null) => void) | undefined;
    vi.mocked(onCloudAuthStateChange).mockImplementationOnce((callback) => {
      emit = callback as typeof emit;
      return { id: "fixture-auth", callback, unsubscribe: vi.fn() };
    });
    vi.mocked(cloudSignUp).mockResolvedValueOnce({ id: "user-1", email: "person@example.test" } as never);
    await act(async () => { root.unmount(); });
    root = createRoot(container);
    await act(async () => { root.render(<AccountSettings />); });
    await fill("cloud.email", "person@example.test");
    await fill("cloud.password", "password-123");
    await act(async () => { button("cloud.signUp").click(); await Promise.resolve(); });
    expect(toast.success).toHaveBeenCalledWith(i18n.t("cloud.confirmEmail"));
    await act(async () => { emit?.("SIGNED_IN", session); await Promise.resolve(); });
    expect(toast.success).toHaveBeenCalledWith(i18n.t("cloud.emailConfirmedSignedIn"));
  });

  it("notifies the standalone dialog after email authentication completes", async () => {
    const authenticated = vi.fn();
    const session = {
      access_token: "fixture-access-token",
      expires_at: 4_102_444_800,
      user: { id: "user-1", email: "person@example.test" },
    } as unknown as Session;
    vi.mocked(cloudSignIn).mockResolvedValueOnce(session);
    await act(async () => { root.render(<AccountSettings onAuthenticated={authenticated} />); });
    await fill("cloud.email", "person@example.test");
    await fill("cloud.password", "password-123");
    await act(async () => { button("cloud.signIn").click(); });
    expect(authenticated).toHaveBeenCalledExactlyOnceWith(session);
  });

  it("keeps a successful login when the optional desktop policy refresh is unavailable", async () => {
    const authenticated = vi.fn();
    const session = {
      access_token: "fixture-access-token",
      expires_at: 4_102_444_800,
      user: { id: "user-1", email: "person@example.test" },
    } as unknown as Session;
    vi.mocked(cloudSignIn).mockResolvedValueOnce(session);
    vi.mocked(refreshCloudPolicy).mockRejectedValueOnce(new Error("TAURI_UNAVAILABLE"));
    await act(async () => { root.render(<AccountSettings onAuthenticated={authenticated} />); });
    await fill("cloud.email", "person@example.test");
    await fill("cloud.password", "password-123");
    await act(async () => {
      button("cloud.signIn").click();
      await Promise.resolve();
    });
    expect(authenticated).toHaveBeenCalledWith(session);
    expect(container.textContent).not.toContain(i18n.t("cloud.accountError"));
  });

  it("starts Google OAuth without requiring email credentials", async () => {
    expect(button("cloud.signInWithGoogle").querySelector('[data-brand-icon="google"]')).not.toBeNull();
    await act(async () => { button("cloud.signInWithGoogle").click(); });
    expect(cloudSignInWithGoogle).toHaveBeenCalledOnce();
    expect(container.querySelector('[role="status"]')?.textContent).toBe(i18n.t("cloud.oauthPending"));
  });

  it("starts GitHub OAuth without requiring email credentials", async () => {
    expect(button("cloud.signInWithGitHub").querySelector('[data-brand-icon="github"]')).not.toBeNull();
    await act(async () => { button("cloud.signInWithGitHub").click(); });
    expect(cloudSignInWithGitHub).toHaveBeenCalledOnce();
    expect(container.querySelector('[role="status"]')?.textContent).toBe(i18n.t("cloud.oauthPending"));
  });
});

describe("cloud sync controls", () => {
  it("shows sync, team, and governance as one horizontal tab list", async () => {
    await renderSignedIn();

    const tabList = container.querySelector(`[role="tablist"][aria-label="${i18n.t("cloud.featureTabs")}"]`);
    const tabs = Array.from(tabList?.querySelectorAll('[role="tab"]') ?? []);
    expect(tabList?.className).toContain("inline-flex");
    expect(tabList?.className).toContain("w-fit");
    expect(tabs.map((tab) => tab.textContent)).toEqual([
      i18n.t("cloud.syncTab"),
      i18n.t("cloud.team"),
      i18n.t("cloud.governance"),
    ]);
    expect(tabs[0]?.getAttribute("aria-selected")).toBe("true");
    expect(tabs[0]?.getAttribute("data-state")).toBe("active");
    expect((tabs[1] as HTMLButtonElement).disabled).toBe(true);

    await act(async () => { (tabs[2] as HTMLButtonElement).click(); });
    expect(tabs[2]?.getAttribute("aria-selected")).toBe("true");
    expect(tabs[2]?.getAttribute("data-state")).toBe("active");
    expect(container.querySelector('[role="tabpanel"]')?.id).toBe("cloud-feature-panel-governance");
  });

  it("reveals team workspace creation only when requested", async () => {
    await renderSignedIn();
    expect(container.querySelector(`[aria-label="${i18n.t("cloud.organizationName")}"]`)).toBeNull();

    await act(async () => { button("cloud.createTeamWorkspace").click(); });
    await fill("cloud.organizationName", "Operations");
    vi.mocked(createOrganization).mockResolvedValueOnce({
      id: "organization-2",
      name: "Operations",
      kind: "team",
      owner_id: "user-1",
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    });
    await act(async () => { button("cloud.createOrganization").click(); });

    expect(createOrganization).toHaveBeenCalledExactlyOnceWith("Operations", "user-1");
    expect(container.textContent).toContain("Operations");
    expect(container.querySelector(`[aria-label="${i18n.t("cloud.organizationName")}"]`)).toBeNull();
    expect(toast.success).toHaveBeenCalledWith(i18n.t("cloud.workspaceCreated"));
  });

  it("reports workspace creation failures with a toast instead of inline text", async () => {
    await renderSignedIn();
    await act(async () => { button("cloud.createTeamWorkspace").click(); });
    await fill("cloud.organizationName", "Operations");
    vi.mocked(createOrganization).mockRejectedValueOnce(new Error("fixture failure"));

    await act(async () => { button("cloud.createOrganization").click(); });

    expect(container.textContent).not.toContain(i18n.t("cloud.error"));
    expect(toast.error).toHaveBeenCalledWith(i18n.t("cloud.error"), { id: "cloud-operation-error" });
  });

  it.each([
    ["", "cloud.syncPassphraseRequired"],
    ["too-short", "cloud.syncPassphraseTooShort"],
  ])("shows validation instead of silently ignoring an invalid passphrase", async (value, errorKey) => {
    await renderSignedIn();
    if (value) await fill("cloud.recoveryPassphrase", value);

    await act(async () => { button("cloud.syncNow").click(); });

    expect(container.querySelector('[role="alert"]')?.textContent).toBe(i18n.t(errorKey));
    expect(document.activeElement).toBe(container.querySelector(`[aria-label="${i18n.t("cloud.recoveryPassphrase")}"]`));
    expect(loadEncryptedInventory).not.toHaveBeenCalled();
  });

  it("uses the device-protected key for daily sync without asking for recovery", async () => {
    vi.mocked(cloudSyncKeyStatus).mockResolvedValue({ configured: true, persistedOnDevice: true, secureStorageAvailable: true });
    vi.mocked(loadEncryptedInventory).mockResolvedValue(null);
    vi.mocked(exportCloudSync).mockResolvedValue({ version: 4, salt: [], nonce: [], ciphertext: [] });
    await renderSignedIn();

    expect(container.querySelector(`[aria-label="${i18n.t("cloud.recoveryPassphrase")}"]`)).toBeNull();
    await act(async () => { button("cloud.syncNow").click(); });

    expect(exportCloudSync).toHaveBeenCalledExactlyOnceWith("organization-1", undefined);
  });

  it("rewraps the cloud key with a new confirmed recovery passphrase", async () => {
    const currentPayload = { version: 4, salt: [1], nonce: [2], ciphertext: [3], keyEnvelope: { salt: [4], nonce: [5], ciphertext: [6] } };
    const rotatedPayload = { ...currentPayload, keyEnvelope: { salt: [7], nonce: [8], ciphertext: [9] } };
    vi.mocked(cloudSyncKeyStatus).mockResolvedValue({ configured: true, persistedOnDevice: true, secureStorageAvailable: true });
    vi.mocked(loadEncryptedInventory).mockResolvedValue({
      id: "sync-1", organization_id: "organization-1", kind: "inventory", logical_id: "organization-1",
      encrypted_payload: currentPayload, revision: 3, updated_by: "user-1", created_at: "", updated_at: "",
    });
    vi.mocked(rotateCloudRecoveryPassphrase).mockResolvedValue(rotatedPayload);
    await renderSignedIn();

    await act(async () => { button("cloud.changeRecoveryPassphrase").click(); });
    await fill("cloud.newRecoveryPassphrase", "the replacement recovery phrase");
    await fill("cloud.confirmRecoveryPassphrase", "the replacement recovery phrase");
    await act(async () => { button("cloud.saveRecoveryPassphrase").click(); });

    expect(rotateCloudRecoveryPassphrase).toHaveBeenCalledExactlyOnceWith(
      "organization-1",
      currentPayload,
      "the replacement recovery phrase",
    );
    expect(writeEncryptedInventory).toHaveBeenCalledExactlyOnceWith("organization-1", rotatedPayload, 3);
    expect(container.textContent).not.toContain(i18n.t("cloud.recoveryPassphraseChanged"));
    expect(toast.success).toHaveBeenCalledWith(i18n.t("cloud.recoveryPassphraseChanged"));
  });

  it("does not rotate the recovery envelope when confirmation differs", async () => {
    vi.mocked(cloudSyncKeyStatus).mockResolvedValue({ configured: true, persistedOnDevice: true, secureStorageAvailable: true });
    await renderSignedIn();
    await act(async () => { button("cloud.changeRecoveryPassphrase").click(); });
    await fill("cloud.newRecoveryPassphrase", "the replacement recovery phrase");
    await fill("cloud.confirmRecoveryPassphrase", "a different recovery phrase");

    await act(async () => { button("cloud.saveRecoveryPassphrase").click(); });

    expect(container.querySelector('[role="alert"]')?.textContent).toBe(i18n.t("cloud.recoveryPassphraseMismatch"));
    expect(rotateCloudRecoveryPassphrase).not.toHaveBeenCalled();
  });

  it("finishes silently instead of showing an empty change preview", async () => {
    vi.mocked(cloudSyncKeyStatus).mockResolvedValue({ configured: true, persistedOnDevice: true, secureStorageAvailable: true });
    vi.mocked(loadEncryptedInventory).mockResolvedValue({
      id: "sync-1",
      organization_id: "organization-1",
      kind: "inventory",
      logical_id: "organization-1",
      encrypted_payload: { version: 4, salt: [], nonce: [], ciphertext: [] },
      revision: 2,
      updated_by: "user-1",
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    });
    vi.mocked(previewCloudSync).mockResolvedValue({
      importId: "import-1",
      groupAdditions: 0,
      groupUpdates: 0,
      profileAdditions: 0,
      profileUpdates: 0,
      groupDeletions: 0,
      profileDeletions: 0,
      localNewer: 0,
      conflicts: 0,
      conflictItems: [],
    });
    await renderSignedIn();

    await act(async () => { button("cloud.syncNow").click(); });

    expect(discardCloudSync).toHaveBeenCalledExactlyOnceWith("import-1");
    expect(button("cloud.applyAndPublish")).toBeUndefined();
    expect(container.textContent).not.toContain(i18n.t("cloud.syncComplete"));
    expect(toast.success).toHaveBeenCalledWith(i18n.t("cloud.syncComplete"));
  });

  it("uses the recovery passphrase only while setting up the device key", async () => {
    vi.mocked(loadEncryptedInventory).mockResolvedValue(null);
    vi.mocked(exportCloudSync).mockResolvedValue({ version: 4, salt: [], nonce: [], ciphertext: [] });
    await renderSignedIn();
    await fill("cloud.recoveryPassphrase", "correct horse battery staple");

    await act(async () => { button("cloud.syncNow").click(); });

    expect(exportCloudSync).toHaveBeenCalledExactlyOnceWith("organization-1", "correct horse battery staple");
  });
});

describe("account workspace summary", () => {
  it("links signed-in users to cloud workspace management", async () => {
    const openCloud = vi.fn();
    vi.mocked(cloudSession).mockResolvedValue({
      access_token: "fixture-access-token",
      expires_at: 4_102_444_800,
      user: { id: "user-1", email: "person@example.test" },
    } as unknown as Session);
    vi.mocked(listOrganizations).mockResolvedValue([
      { id: "organization-1", name: "Personal", kind: "personal", owner_id: "user-1", created_at: "", updated_at: "" },
      { id: "organization-2", name: "Operations", kind: "team", owner_id: "user-1", created_at: "", updated_at: "" },
    ]);
    await act(async () => { root.unmount(); });
    root = createRoot(container);
    await act(async () => {
      root.render(<AccountSettings onOpenCloud={openCloud} />);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(container.textContent).toContain(i18n.t("cloud.workspaceSummary", { count: 2 }));
    await act(async () => { button("cloud.manageWorkspaces").click(); });
    expect(openCloud).toHaveBeenCalledOnce();
  });
});
