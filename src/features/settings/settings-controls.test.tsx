// @vitest-environment jsdom
import { act, type ComponentProps, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import i18n from "../../i18n";
import { useSettingsStore } from "../../stores/settings-store";
import { getMyMembership, listOrganizationMembers, listMyOrganizationInvites, listOrganizationInvites, listAccessPolicies, listCloudAuditRecords, updateOrganizationMemberRole, createOrganizationInvite, createAccessPolicy } from "../../lib/supabase/cloud";
import { SettingsPanel } from "./SettingsPanel";
import { ModelProfileForm } from "./ModelProfileForm";
import { CloudTeamPanel } from "./CloudTeamPanel";
import { CloudGovernancePanel } from "./CloudGovernancePanel";
import { advancedPresets, providerLabelKeys } from "./model-provider-presets";
import type { AdvancedProviderKind } from "../../types/agentic";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../lib/supabase/client", () => ({ cloudConfigured: false, cloudEndpoint: null, cloudPublishableKey: null }));
vi.mock("../../lib/supabase/cloud", () => ({
  cloudOAuthErrorEvent: "runory:cloud-oauth-error",
  cloudSignInWithGitHub: vi.fn(),
  getMyMembership: vi.fn(), listOrganizationMembers: vi.fn(), listOrganizationInvites: vi.fn().mockResolvedValue([]), listMyOrganizationInvites: vi.fn().mockResolvedValue([]),
  createOrganizationInvite: vi.fn(), updateOrganizationMemberRole: vi.fn(), acceptOrganizationInvite: vi.fn(), removeOrganizationMember: vi.fn(), revokeOrganizationInvite: vi.fn(),
  listAccessPolicies: vi.fn().mockResolvedValue([]), listCloudAuditRecords: vi.fn().mockResolvedValue([]), createAccessPolicy: vi.fn(), deleteAccessPolicy: vi.fn(), cloudSession: vi.fn(),
}));
// Keep real selection handlers; jsdom cannot perform popper collision layout.
vi.mock("../../components/ui/select", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../components/ui/select")>();
  return { ...actual, SelectContent: (props: ComponentProps<typeof actual.SelectContent>) => <actual.SelectContent {...props} position="item-aligned" /> };
});

let root: Root;
let container: HTMLDivElement;
async function render(node: ReactNode) { await act(async () => { root.render(node); }); }
async function click(element: HTMLElement) { await act(async () => { element.click(); }); }
function byLabel<T extends HTMLElement>(key: string): T {
  const text = i18n.t(key);
  const direct = Array.from(document.querySelectorAll<HTMLElement>("[aria-label]")).find((item) => item.getAttribute("aria-label") === text);
  const label = Array.from(document.querySelectorAll("label")).find((item) => item.textContent === text);
  const element = direct ?? (label && document.getElementById(label.htmlFor));
  if (!element) throw new Error(`Missing label: ${key}`);
  return element as T;
}
function button(key: string): HTMLButtonElement {
  const element = Array.from(document.querySelectorAll("button")).find((item) => item.textContent === i18n.t(key));
  if (!element) throw new Error(`Missing button: ${key}`);
  return element;
}
async function fill(key: string, value: string) {
  const input = byLabel<HTMLInputElement>(key);
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}
async function choose(key: string, optionKey: string, options?: string[]) {
  const trigger = byLabel<HTMLButtonElement>(key);
  expect(trigger.getAttribute("role")).toBe("combobox");
  await act(async () => { trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })); });
  const items = Array.from(document.querySelectorAll<HTMLElement>('[role="option"]'));
  if (options) expect(items.map((item) => item.textContent)).toEqual(options.map((item) => i18n.t(item)));
  const item = items.find((candidate) => candidate.textContent === i18n.t(optionKey));
  if (!item) throw new Error(`Missing option: ${optionKey}`);
  await click(item);
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(listMyOrganizationInvites).mockResolvedValue([]);
  vi.mocked(listOrganizationInvites).mockResolvedValue([]);
  vi.mocked(listAccessPolicies).mockResolvedValue([]);
  vi.mocked(listCloudAuditRecords).mockResolvedValue([]);
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "known_host_list") return [];
    if (command === "credential_status") return { vaultInitialized: true, vaultUnlocked: false, platformUnlockConfigured: false, platformUnlockAvailable: false };
    if (command === "settings_get") {
      const state = useSettingsStore.getState();
      return { theme: state.theme, language: state.language };
    }
    if (command === "settings_update") {
      const state = useSettingsStore.getState();
      return { theme: state.theme, language: state.language, ...(args as { request: object }).request };
    }
    return { enabled: false, authenticated: false };
  });
  vi.mocked(getMyMembership).mockResolvedValue({ organization_id: "org", user_id: "me", role: "owner", created_at: "" });
  vi.mocked(listOrganizationMembers).mockResolvedValue([{ user_id: "member", email: "fixture@example.test", role: "viewer", created_at: "" }]);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(180);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(36);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, top: 0, left: 0, right: 180, bottom: 36, width: 180, height: 36, toJSON() {} });
  const style = document.createElement("div").style;
  style.cssText = "position: static; display: block; visibility: visible; direction: ltr; width: 180px; height: 36px; transform: none; filter: none; perspective: none;";
  vi.spyOn(window, "getComputedStyle").mockReturnValue(style);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe.each(["en-US", "zh-CN"])("settings controls (%s)", (language) => {
  beforeEach(async () => {
    useSettingsStore.setState({ theme: "system", language: language as "en-US" | "zh-CN", saving: false, persistenceError: false });
    await i18n.changeLanguage(language);
  });
  it("persists theme/language choices through the existing settings command", async () => {
    await render(<SettingsPanel onClose={() => {}} />);
    await choose("settings.theme", "settings.dark", ["settings.system", "settings.light", "settings.dark"]);
    expect(useSettingsStore.getState().theme).toBe("dark");
    await choose("settings.language", language === "en-US" ? "settings.chinese" : "settings.english");
    expect(useSettingsStore.getState().language).toBe(language === "en-US" ? "zh-CN" : "en-US");
    expect(vi.mocked(invoke).mock.calls.filter(([name]) => name === "settings_update")).toHaveLength(2);
  });
  it("restores persisted settings and reports a failed write", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "known_host_list") return [];
      if (command === "credential_status") return { vaultInitialized: true, vaultUnlocked: false, platformUnlockConfigured: false, platformUnlockAvailable: false };
      if (command === "settings_get") return { theme: "system", language };
      if (command === "settings_update") throw new Error("STORAGE_ERROR");
      return { enabled: false, authenticated: false };
    });
    await render(<SettingsPanel onClose={() => {}} />);
    await choose("settings.theme", "settings.dark", ["settings.system", "settings.light", "settings.dark"]);
    await act(async () => { await useSettingsStore.getState().setTheme("dark"); });
    expect(useSettingsStore.getState().theme).toBe("system");
    expect(document.body.textContent).toContain(i18n.t("settings.persistenceError"));
  });
  it("keeps account management separate from cloud workspace settings", async () => {
    await render(<SettingsPanel onClose={() => {}} />);
    expect(button("settings.section.account")).toBeTruthy();
    expect(button("settings.section.cloud")).toBeTruthy();
    await click(button("settings.section.account"));
    await act(async () => { await import("./AccountSettings"); });
    expect(document.querySelector(".settings-content-header h3")?.textContent).toBe(i18n.t("settings.section.account"));
    expect(document.body.textContent).toContain(i18n.t("cloud.accountUnavailable"));
    expect(document.body.textContent?.toLowerCase()).not.toMatch(/rust|supabase|stronghold/);
  });
  it("keeps the vault password transient and clears the input after unlocking", async () => {
    await render(<SettingsPanel onClose={() => {}} />);
    await click(button("settings.section.security"));
    const password = byLabel<HTMLInputElement>("settings.vaultPassword");
    expect(password.type).toBe("password");
    await fill("settings.vaultPassword", "fixture-only-password");
    await click(button("settings.unlockVault"));
    expect(invoke).toHaveBeenCalledWith("vault_unlock", { request: { masterPassword: "fixture-only-password" } });
    expect(password.value).toBe("");
  });
  it("resets provider-specific fields and secrets, while allowing a custom model", async () => {
    const save = vi.fn().mockResolvedValue(undefined);
    await render(<ModelProfileForm profile={null} busy={false} onSave={save} onCancel={() => {}} />);
    await fill("settings.modelApiKey", "fixture-only-key");
    await click(byLabel("settings.showApiKey"));
    await choose("settings.modelProvider", providerLabelKeys["open-ai-compatible"], (Object.keys(advancedPresets) as AdvancedProviderKind[]).map((kind) => providerLabelKeys[kind]));
    expect(byLabel<HTMLInputElement>("settings.modelApiKey").value).toBe("");
    expect(byLabel<HTMLInputElement>("settings.modelApiKey").type).toBe("password");
    expect(byLabel<HTMLInputElement>("settings.modelBaseUrl").readOnly).toBe(false);
    await fill("settings.modelBaseUrl", "https://fixture.example.test/v1");
    await fill("settings.modelLabel", "custom-model");
    await fill("settings.modelApiKey", "fixture-new-key");
    await click(document.querySelector<HTMLButtonElement>('button[type="submit"]')!);
    expect(save).toHaveBeenCalledWith(expect.objectContaining({ kind: "open-ai-compatible", model: "custom-model", baseUrl: "https://fixture.example.test/v1", apiKey: "fixture-new-key" }));
    await render(<ModelProfileForm profile={null} busy onSave={save} onCancel={() => {}} />);
    expect(byLabel<HTMLButtonElement>("settings.modelProvider").disabled).toBe(true);
    expect(byLabel<HTMLButtonElement>("settings.models.suggestions").disabled).toBe(true);
  });
  it("preserves role choices and sends only the selected role to the existing service", async () => {
    await render(<CloudTeamPanel organizationId="org" userId="me" onMembershipChanged={async () => {}} />);
    await choose("cloud.memberRole", "cloud.role.operator", ["cloud.role.admin", "cloud.role.operator", "cloud.role.viewer"]);
    expect(updateOrganizationMemberRole).toHaveBeenCalledExactlyOnceWith("org", "member", "operator");
    await choose("cloud.inviteRole", "cloud.role.viewer");
    await fill("cloud.inviteEmail", "invite@example.test");
    expect(createOrganizationInvite).not.toHaveBeenCalled();
    await click(button("cloud.sendInvite"));
    expect(createOrganizationInvite).toHaveBeenCalledExactlyOnceWith("org", "invite@example.test", "viewer");
  });
  it("does not expose management controls to a viewer", async () => {
    vi.mocked(getMyMembership).mockResolvedValue({ organization_id: "org", user_id: "me", role: "viewer", created_at: "" });
    await render(<CloudTeamPanel organizationId="org" userId="me" onMembershipChanged={async () => {}} />);
    expect(document.querySelector('[role="combobox"]')).toBeNull();
    await render(<CloudGovernancePanel organizationId="org" userId="me" />);
    expect(document.querySelector('[role="combobox"]')).toBeNull();
    expect(createAccessPolicy).not.toHaveBeenCalled();
  });
  it("does not let an admin change owners or other admins", async () => {
    vi.mocked(getMyMembership).mockResolvedValue({ organization_id: "org", user_id: "me", role: "admin", created_at: "" });
    vi.mocked(listOrganizationMembers).mockResolvedValue([
      { user_id: "owner", email: "owner@example.test", role: "owner", created_at: "" },
      { user_id: "admin", email: "admin@example.test", role: "admin", created_at: "" },
      { user_id: "member", email: "member@example.test", role: "viewer", created_at: "" },
    ]);
    await render(<CloudTeamPanel organizationId="org" userId="me" onMembershipChanged={async () => {}} />);
    expect(document.querySelectorAll(`[aria-label="${i18n.t("cloud.memberRole")}"]`)).toHaveLength(1);
    await choose("cloud.memberRole", "cloud.role.operator", ["cloud.role.operator", "cloud.role.viewer"]);
    expect(updateOrganizationMemberRole).toHaveBeenCalledExactlyOnceWith("org", "member", "operator");
  });
  it("keeps policy effects and actions bound to the existing create action", async () => {
    await render(<CloudGovernancePanel organizationId="org" userId="me" />);
    await fill("cloud.policyName", "Fixture policy");
    await choose("cloud.policyEffect", "cloud.effect.allow", ["cloud.effect.deny", "cloud.effect.allow"]);
    await choose("cloud.policyAction", "cloud.action.deploy", ["connect", "read-files", "write-files", "operate", "deploy", "ai-execute"].map((value) => `cloud.action.${value}`));
    expect(createAccessPolicy).not.toHaveBeenCalled();
    await click(byLabel("cloud.createPolicy"));
    expect(createAccessPolicy).toHaveBeenCalledExactlyOnceWith("org", "Fixture policy", "allow", "deploy");
  });
});
