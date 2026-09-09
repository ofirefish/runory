// @vitest-environment jsdom
import { act, type ComponentProps } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import * as api from "../../lib/tauri/tunnels";
import * as ssh from "../../lib/tauri/ssh";
import { useCatalogStore } from "../../stores/catalog-store";
import { useSessionStore } from "../../stores/session-store";
import type { TunnelView } from "../../types/tunnels";
import type { SelectControl } from "../../components/ui/select-control";
import { TunnelsPage } from "./TunnelsPage";

vi.mock("../../lib/tauri/tunnels", () => ({ listTunnels: vi.fn(), saveTunnel: vi.fn(), startTunnel: vi.fn(), connectStartTunnel: vi.fn(), stopTunnel: vi.fn(), checkTunnel: vi.fn(), deleteTunnel: vi.fn() }));
vi.mock("../../components/ui/select-control", () => ({ SelectControl: ({ value, onValueChange, options, label, id, disabled }: ComponentProps<typeof SelectControl>) => <select id={id} aria-label={label} value={value} disabled={disabled} onChange={(event) => onValueChange(event.target.value)}>{options.map((item) => <option key={item.value} value={item.value}>{item.label}</option>)}</select> }));
vi.mock("../../lib/tauri/ssh", () => ({
  credentialStatus: vi.fn(), prepareHostVerification: vi.fn(), trustHost: vi.fn(), cancelHostVerification: vi.fn(),
  testSsh: vi.fn(), forgetCredential: vi.fn(), initializeVault: vi.fn(), unlockVault: vi.fn(), unlockVaultWithPlatform: vi.fn(),
}));
const vault = { vaultInitialized: true, vaultUnlocked: true, hasCredential: false, platformUnlockSupported: true, platformUnlockAvailable: true, platformUnlockConfigured: true };
const verification = { attemptId: "verified-attempt", host: "gateway.test", port: 22, keyType: "ssh-ed25519", fingerprint: "SHA256:fixture", status: "trusted" as const };
const profile = { id: "profile", name: "Gateway", host: "gateway.test", port: 22, username: "user", authMethod: "password" as const, connectionRoute: { type: "direct" as const }, groupId: null, sortOrder: 0, createdAt: "", updatedAt: "" };
const view = (id: string, running = false): TunnelView => ({
  rule: { id, name: `Database ${id}`, profileId: profile.id, targetHost: "db.internal", targetPort: 5432, localPort: id === "a" ? 15432 : 25432 },
  status: { state: running ? "running" : "stopped", sessionId: running ? "session" : null, startedAt: null, checkedAt: null, health: "unchecked", errorCode: null, activeConnections: 0, bytesSent: 0, bytesReceived: 0, events: [] },
});
let root: Root;
let container: HTMLDivElement;
const onConnectProfile = vi.fn();
const onShowSession = vi.fn();
const byText = (text: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === text)!;
async function click(button: HTMLButtonElement) { expect(button).toBeTruthy(); await act(async () => { button.click(); }); }
async function change(selector: string, value: string) {
  const input = document.querySelector<HTMLInputElement>(selector)!;
  await act(async () => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value); input.dispatchEvent(new Event("input", { bubbles: true })); });
}
async function render(initialProfileId?: string) { await act(async () => { root.render(<TunnelsPage initialProfileId={initialProfileId} onConnectProfile={onConnectProfile} onShowSession={onShowSession} />); }); }
beforeEach(async () => {
  vi.clearAllMocks(); vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); await i18n.changeLanguage("en-US");
  useCatalogStore.setState({ profiles: [profile] });
  useSessionStore.setState({ tabs: [{ id: "tab", profileId: profile.id, connectionAttemptId: "attempt", sessionId: "session", state: "connected", view: "terminal" }], activeTabId: "tab" });
  vi.mocked(api.listTunnels).mockResolvedValue([view("a"), view("b", true)]);
  vi.mocked(api.startTunnel).mockResolvedValue();
  vi.mocked(api.connectStartTunnel).mockResolvedValue({ credentialSaved: true });
  vi.mocked(ssh.credentialStatus).mockResolvedValue(vault);
  vi.mocked(ssh.prepareHostVerification).mockResolvedValue(verification);
  vi.mocked(ssh.trustHost).mockResolvedValue();
  vi.mocked(ssh.cancelHostVerification).mockResolvedValue();
  container = document.createElement("div"); document.body.append(container); root = createRoot(container);
});
afterEach(async () => { await act(async () => root.unmount()); container.remove(); vi.unstubAllGlobals(); });

describe("tunnel management", () => {
  it("keeps server sections expanded without group disclosure controls", async () => {
    await render();
    expect(container.querySelectorAll("section.tunnel-group")).toHaveLength(1);
    expect(container.querySelector(".tunnel-group")?.getAttribute("aria-label")).toBe("Gateway");
    expect(container.querySelector("details.tunnel-group, .tunnel-group > summary")).toBeNull();
    expect(container.querySelectorAll("tbody tr")).toHaveLength(2);
  });
  it("derives overview counts from all rules, independently of list filters", async () => {
    const active = view("b", true);
    active.status.activeConnections = 3;
    const failed = view("c");
    failed.status.state = "error";
    failed.status.health = "unreachable";
    vi.mocked(api.listTunnels).mockResolvedValue([view("a"), active, failed]);
    await render();
    const counts = () => [...container.querySelectorAll(".tunnel-overview dd")].map((item) => item.textContent);
    expect(counts()).toEqual(["3", "1", "3", "1"]);
    await change(".tunnel-search input", "Database a");
    expect(counts()).toEqual(["3", "1", "3", "1"]);
  });
  it("does not present unavailable overview metrics as zero", async () => {
    vi.mocked(api.listTunnels).mockRejectedValue({ code: "UNKNOWN" });
    await render();
    expect([...container.querySelectorAll(".tunnel-overview dd")].map((item) => item.textContent)).toEqual(Array(4).fill(i18n.t("tunnels.notYet")));
  });
  it("separates active listener from unchecked destination and filters rules", async () => {
    await render();
    expect(container.textContent).toContain(i18n.t("tunnels.state.running"));
    expect(container.textContent).toContain(i18n.t("tunnels.health.unchecked"));
    expect(container.textContent).not.toContain(i18n.t("tunnels.health.reachable"));
    await change(".tunnel-search input", "Database a");
    expect(container.querySelectorAll("tbody tr")).toHaveLength(1);
    expect(api.checkTunnel).not.toHaveBeenCalled();
  });
  it("starts a background connection by rule ID and stops independently", async () => {
    await render();
    await click(container.querySelector('[aria-label="Start Database a"]')!);
    expect(api.startTunnel).toHaveBeenCalledWith("a");
    await click(container.querySelector('[aria-label="Stop Database b"]')!);
    expect(api.stopTunnel).toHaveBeenCalledWith("b");
    expect(onConnectProfile).not.toHaveBeenCalled();
  });
  it("authenticates offline forwarding in place without creating a workspace tab", async () => {
    useSessionStore.setState({ tabs: [], activeTabId: null });
    vi.mocked(api.startTunnel).mockRejectedValueOnce({ code: "TUNNEL_CONNECTION_REQUIRED" });
    await render();
    await click(container.querySelector('[aria-label="Start Database a"]')!);
    expect(document.body.textContent).toContain(i18n.t("tunnels.backgroundHint"));
    await change('input[name="password"]', "one-time-secret");
    await click(byText(i18n.t("tunnels.authenticateStart")));
    expect(api.connectStartTunnel).toHaveBeenCalledWith("a", {
      profileId: "profile", verificationAttemptId: "verified-attempt",
      credential: { mode: "session-only", secret: "one-time-secret" },
    });
    expect(onConnectProfile).not.toHaveBeenCalled();
    expect(onShowSession).not.toHaveBeenCalled();
    expect(useSessionStore.getState().tabs).toEqual([]);
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });
  it("does not prompt for terminal selection even with multiple terminal sessions", async () => {
    const first = useSessionStore.getState().tabs[0];
    useSessionStore.setState({ tabs: [first, { ...first, id: "tab2", sessionId: "session2" }] });
    await render(); await click(container.querySelector('[aria-label="Start Database a"]')!);
    expect(api.startTunnel).toHaveBeenCalledWith("a");
    expect(document.querySelector('[aria-label="SSH session"]')).toBeNull();
    expect(useSessionStore.getState().tabs).toHaveLength(2);
    expect(onConnectProfile).not.toHaveBeenCalled();
  });
  it("uses stored credentials without exposing secrets or opening a terminal", async () => {
    vi.mocked(api.startTunnel).mockRejectedValueOnce({ code: "TUNNEL_CONNECTION_REQUIRED" });
    vi.mocked(ssh.credentialStatus).mockResolvedValue({ ...vault, hasCredential: true });
    await render(); await click(container.querySelector('[aria-label="Start Database a"]')!);
    expect(api.connectStartTunnel).toHaveBeenCalledWith("a", {
      profileId: "profile", verificationAttemptId: "verified-attempt", credential: { mode: "stored" },
    });
    expect(onConnectProfile).not.toHaveBeenCalled();
  });
  it("unlocks stored credentials in place before authenticating background forwarding", async () => {
    vi.mocked(api.startTunnel).mockRejectedValueOnce({ code: "TUNNEL_CONNECTION_REQUIRED" });
    vi.mocked(ssh.credentialStatus).mockResolvedValue({ ...vault, hasCredential: true })
      .mockResolvedValueOnce({ ...vault, vaultUnlocked: false, hasCredential: true });
    await render(); await click(container.querySelector('[aria-label="Start Database a"]')!);
    expect(api.connectStartTunnel).not.toHaveBeenCalled();
    await click(byText(i18n.t("connection.unlockVault")));
    expect(ssh.unlockVaultWithPlatform).toHaveBeenCalledTimes(1);
    expect(api.connectStartTunnel).toHaveBeenCalledWith("a", {
      profileId: "profile", verificationAttemptId: "verified-attempt", credential: { mode: "stored" },
    });
    expect(onConnectProfile).not.toHaveBeenCalled();
  });
  it("requires explicit trust for an unknown host and supports cancellation", async () => {
    vi.mocked(api.startTunnel).mockRejectedValueOnce({ code: "TUNNEL_CONNECTION_REQUIRED" });
    vi.mocked(ssh.prepareHostVerification).mockResolvedValue({ ...verification, status: "unknown" });
    await render(); await click(container.querySelector('[aria-label="Start Database a"]')!);
    await change('input[name="password"]', "one-time-secret");
    await click(byText(i18n.t("tunnels.authenticateStart")));
    expect(api.connectStartTunnel).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain("SHA256:fixture");
    await click(byText(i18n.t("connection.cancel")));
    expect(ssh.cancelHostVerification).toHaveBeenCalledWith("verified-attempt");
    expect(api.connectStartTunnel).not.toHaveBeenCalled();
    await click(byText(i18n.t("tunnels.authenticateStart")));
    await click(byText(i18n.t("connection.trustOnce")));
    expect(ssh.trustHost).toHaveBeenCalledWith("verified-attempt", false);
    expect(api.connectStartTunnel).toHaveBeenCalledTimes(1);
  });
  it("blocks changed host keys before starting the background connection", async () => {
    vi.mocked(api.startTunnel).mockRejectedValueOnce({ code: "TUNNEL_CONNECTION_REQUIRED" });
    vi.mocked(ssh.prepareHostVerification).mockRejectedValueOnce({ code: "HOST_KEY_CHANGED" });
    await render(); await click(container.querySelector('[aria-label="Start Database a"]')!);
    await change('input[name="password"]', "one-time-secret");
    await click(byText(i18n.t("tunnels.authenticateStart")));
    expect(api.connectStartTunnel).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain(i18n.t("connection.changedTitle"));
    expect(onConnectProfile).not.toHaveBeenCalled();
  });
  it("saves without starting and pre-fills the server shortcut", async () => {
    vi.mocked(api.saveTunnel).mockResolvedValue(view("a").rule);
    await render(profile.id);
    expect(document.querySelector<HTMLSelectElement>("#tunnel-profile")?.value).toBe(profile.id);
    await change("#tunnel-name", "Private DB"); await change("#tunnel-host", "db.internal");
    await change("#tunnel-target-port", "5432"); await change("#tunnel-local-port", "15432");
    await act(async () => { document.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); });
    expect(api.saveTunnel).toHaveBeenCalledWith({ name: "Private DB", profileId: "profile", targetHost: "db.internal", targetPort: 5432, localPort: 15432, id: undefined });
    expect(api.startTunnel).not.toHaveBeenCalled();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });
  it("leaves new forwarding inputs empty and requires both ports before saving", async () => {
    await render(profile.id);
    for (const id of ["tunnel-name", "tunnel-host", "tunnel-target-port", "tunnel-local-port"]) {
      expect(document.querySelector<HTMLInputElement>(`#${id}`)?.value).toBe("");
    }
    await change("#tunnel-name", "Private DB"); await change("#tunnel-host", "db.internal");
    await act(async () => { document.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); });
    expect(api.saveTunnel).not.toHaveBeenCalled();
    expect(document.querySelector("#tunnel-target-port")?.getAttribute("aria-invalid")).toBe("true");
    expect(document.querySelector("#tunnel-local-port")?.getAttribute("aria-invalid")).toBe("true");
  });
  it("retains saved values when editing an existing forwarding rule", async () => {
    await render();
    const row = [...container.querySelectorAll("tbody tr")].find((item) => item.textContent?.includes("Database a"))!;
    await click([...row.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === i18n.t("common.edit"))!);
    expect(document.querySelector<HTMLInputElement>("#tunnel-target-port")?.value).toBe("5432");
    expect(document.querySelector<HTMLInputElement>("#tunnel-local-port")?.value).toBe("15432");
  });
  it("localizes errors without rendering backend text and retains the listener health distinction", async () => {
    vi.mocked(api.startTunnel).mockRejectedValue({ code: "TUNNEL_PORT_IN_USE", message: "untrusted content" });
    await render(); await click(container.querySelector('[aria-label="Start Database a"]')!);
    expect(container.textContent).toContain(i18n.t("errors.TUNNEL_PORT_IN_USE"));
    expect(container.textContent).not.toContain("untrusted content");
  });
  it("disables edit/delete on running rules and renders metadata-only detail", async () => {
    await render(); await click(byText("Database b"));
    expect(container.querySelector(".tunnel-details")?.textContent).toContain(i18n.t("tunnels.healthHint"));
    const route = container.querySelector(".tunnel-route")!;
    expect(route.querySelectorAll("li")).toHaveLength(3);
    expect(route.textContent).toContain("127.0.0.1:25432");
    expect(route.textContent).toContain("Gateway");
    expect(route.textContent).toContain("db.internal:5432");
    const row = [...container.querySelectorAll("tbody tr")].find((item) => item.textContent?.includes("Database b"))!;
    expect([...row.querySelectorAll<HTMLButtonElement>(".item-menu-popover button")].filter((button) => button.disabled)).toHaveLength(2);
  });
});
