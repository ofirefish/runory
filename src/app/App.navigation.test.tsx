// @vitest-environment jsdom
import { act, type ComponentProps } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../i18n";
import { useCatalogStore } from "../stores/catalog-store";
import { useSessionStore } from "../stores/session-store";
import { connectSsh, disconnectSsh } from "../lib/tauri/ssh";
import { listTunnels, sessionTunnelImpact } from "../lib/tauri/tunnels";
import type { ConnectionDialog } from "../features/sessions/ConnectionDialog";
import type { TerminalView } from "../features/terminal/TerminalView";
import type { ContextDock } from "../components/layout/ContextDock";
import type { ContextPanel } from "../components/context-panel/ContextPanel";
import { App } from "./App";

const terminal = vi.hoisted(() => ({ write: vi.fn(), mounted: vi.fn(), disposed: vi.fn() }));
vi.mock("../components/ui/popover", async (importOriginal) => {
  // Collision positioning is browser-verified; jsdom has no viewport layout.
  const actual = await importOriginal<typeof import("../components/ui/popover")>();
  return { ...actual, PopoverContent: (props: ComponentProps<typeof actual.PopoverContent>) => <actual.PopoverContent {...props} avoidCollisions={false} /> };
});
vi.mock("../hooks/use-theme", () => ({ useTheme: () => undefined }));
vi.mock("../hooks/use-language", () => ({ useLanguage: () => undefined }));
vi.mock("../components/layout/WindowControls", () => ({ WindowControls: ({ onOpenPricing }: { onOpenPricing: () => void }) => <button type="button" aria-label="window.openPricing" onClick={onOpenPricing} /> }));
vi.mock("../features/mobile/MobilePrivacyGuard", () => ({ MobilePrivacyGuard: () => null }));
vi.mock("../components/context-panel/ContextPanel", () => ({ ContextPanel: ({ onNewTerminal }: ComponentProps<typeof ContextPanel>) => <aside data-testid="context-panel"><button onClick={onNewTerminal}>new terminal fixture</button></aside> }));
vi.mock("../components/layout/ContextDock", () => ({ ContextDock: ({ terminalContent, active }: ComponentProps<typeof ContextDock>) => <div data-testid="dock" data-active={active}>{terminalContent(null)}</div> }));
vi.mock("../features/terminal/TerminalView", async () => {
  const { forwardRef, useEffect, useImperativeHandle } = await import("react");
  return { TerminalView: forwardRef(function TestTerminal({ active }: ComponentProps<typeof TerminalView>, ref) {
    useImperativeHandle(ref, () => ({ write: terminal.write, dimensions: () => ({ cols: 120, rows: 34 }) }));
    useEffect(() => { terminal.mounted(); return () => { terminal.disposed(); }; }, []);
    return <div data-testid="terminal" data-active={active} />;
  }) };
});
vi.mock("../features/sessions/ConnectionDialog", () => ({ ConnectionDialog: ({ profile, onConnect, onClose }: ComponentProps<typeof ConnectionDialog>) => <div role="dialog" aria-label="connection"><span>{profile.name}</span><button onClick={() => void onConnect({ profileId: profile.id, verificationAttemptId: "fixture-verification", credential: { mode: "session-only", secret: "" } }).then(onClose)}>connect fixture</button></div> }));
vi.mock("../features/settings/SettingsPanel", () => ({ SettingsPanel: ({ onClose, initialSection }: { onClose: () => void; initialSection?: string }) => <div role="dialog" aria-label="settings" data-section={initialSection}><button onClick={onClose}>close settings</button></div> }));
vi.mock("../features/settings/PricingDialog", () => ({ PricingDialog: ({ onClose }: { onClose: () => void }) => <div role="dialog" aria-label="pricing"><button onClick={onClose}>close pricing</button></div> }));
vi.mock("../features/auth/AuthDialog", () => ({ AuthDialog: ({ onClose }: { onClose: () => void }) => <div role="dialog" aria-label="auth"><button onClick={onClose}>close auth</button></div> }));vi.mock("../features/profiles/ProfileDialog", () => ({ ProfileDialog: ({ onClose }: { onClose: () => void }) => <div role="dialog" aria-label="profile"><button onClick={onClose}>close profile</button></div> }));
vi.mock("../lib/tauri/ssh", () => ({ connectSsh: vi.fn(), disconnectSsh: vi.fn(), reconnectSsh: vi.fn(), testSsh: vi.fn() }));
vi.mock("../lib/tauri/tunnels", () => ({ listTunnels: vi.fn().mockResolvedValue([]), sessionTunnelImpact: vi.fn().mockResolvedValue([]) }));

let container: HTMLDivElement;
let root: Root;
const profiles = ["alpha", "beta"].map((name) => ({ id: name, name, host: `${name}.example.test`, port: 22, username: "root", groupId: null, authMethod: "password" as const, connectionRoute: { type: "direct" as const }, sortOrder: 0, createdAt: "", updatedAt: "" }));
async function click(selector: string) {
  const element = container.querySelector<HTMLButtonElement>(selector);
  expect(element).not.toBeNull();
  await act(async () => { element?.click(); });
}
const nav = (key: string) => `.primary-rail button[title="${i18n.t(key)}"]`;
const workspace = () => container.querySelector<HTMLElement>(".workspace-shell")!;

beforeEach(async () => {
  vi.clearAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, top: 0, left: 0, right: 280, bottom: 32, width: 280, height: 32, toJSON() {} });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(280);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(32);
  const layoutStyle = document.createElement("div").style;
  layoutStyle.cssText = "position: static; display: block; visibility: visible; direction: ltr; width: 280px; height: 32px; transform: none; filter: none; perspective: none;";
  vi.spyOn(window, "getComputedStyle").mockReturnValue(layoutStyle);
  vi.mocked(listTunnels).mockResolvedValue([]);
  vi.mocked(sessionTunnelImpact).mockResolvedValue([]);
  localStorage.clear();
  await i18n.changeLanguage("en-US");
  useCatalogStore.setState({ profiles, groups: [], selectedProfileId: null, loading: false, errorCode: null, load: vi.fn().mockResolvedValue(undefined) });
  useSessionStore.setState({ tabs: [], activeTabId: null });
  vi.mocked(connectSsh).mockResolvedValue({ sessionId: "session-alpha", credentialSaved: false });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () => { root.render(<App />); });
});
afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("server management and session navigation", () => {
  it("opens tunnels without disposing terminals and pre-fills the server shortcut", async () => {
    await click(".host-select");
    await click(".server-profile-actions button");
    await click('[aria-label="connection"] button');
    const instance = container.querySelector('[data-testid="terminal"]');
    await click(nav("tunnels.nav"));
    expect(container.querySelector(".tunnels-page")).not.toBeNull();
    expect(container.querySelector('[data-testid="terminal"]')).toBe(instance);
    expect(terminal.disposed).not.toHaveBeenCalled();
    expect(disconnectSsh).not.toHaveBeenCalled();
    await click(nav("sidebar.servers"));
    expect(container.querySelector(".host-menu .lucide-cable")).not.toBeNull();
    const shortcut = [...container.querySelectorAll<HTMLButtonElement>(".server-profile-actions button")].find((button) => button.textContent === i18n.t("tunnels.create"))!;
    await act(async () => shortcut.click());
    expect(container.querySelector(".tunnels-page")).not.toBeNull();
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain(i18n.t("tunnels.create"));
    expect(document.querySelector("#tunnel-profile")?.textContent).toContain("alpha");
  });

  it("shows the catalog in management and selects details without connecting", async () => {
    expect(container.querySelector(".server-management")).not.toBeNull();
    expect(workspace().style.display).toBe("none");
    expect(container.querySelector(nav("shell.sessions"))).toBeNull();
    await click(".host-select");
    expect(container.querySelector(".server-profile-details")?.textContent).toContain("alpha.example.test");
    expect(connectSsh).not.toHaveBeenCalled();
    expect(container.querySelector('[aria-label="connection"]')).toBeNull();
  });

  it("keeps terminals and channel output alive across page switches and reuses connected sessions", async () => {
    await click(".host-select");
    await click(".server-profile-actions button");
    expect(workspace().style.display).toBe("");
    await click('[aria-label="connection"] button');
    const instance = container.querySelector('[data-testid="terminal"]');
    expect(terminal.mounted).toHaveBeenCalledTimes(1);
    expect(instance?.getAttribute("data-active")).toBe("true");
    await click(nav("sidebar.servers"));
    expect(instance?.getAttribute("data-active")).toBe("false");
    const channel = vi.mocked(connectSsh).mock.calls[0][1];
    await act(async () => { channel({ event: "output", data: { bytes: [65, 66] } }); });
    expect(terminal.write).toHaveBeenCalledWith([65, 66]);
    expect(terminal.disposed).not.toHaveBeenCalled();
    await click(".server-profile-actions button");
    expect(container.querySelector('[data-testid="terminal"]')).toBe(instance);
    expect(instance?.getAttribute("data-active")).toBe("true");
    expect(container.querySelector('[aria-label="connection"]')).toBeNull();
    expect(connectSsh).toHaveBeenCalledTimes(1);
    expect(disconnectSsh).not.toHaveBeenCalled();
    expect(useSessionStore.getState().tabs).toHaveLength(1);
    await click(nav("sidebar.servers"));
    await click(".host-item:nth-child(2) .host-select");
    await click('.global-topbar .workspace-tab-select[title="alpha"]');
    await click('[data-testid="context-panel"] button');
    expect(container.querySelector('[aria-label="connection"] span')?.textContent).toBe("alpha");
  });

  it("opens settings from either page and navigates to server management", async () => {
    await click(".host-select");
    await click(".server-profile-actions button");
    await click('[aria-label="connection"] button');
    for (const selector of [nav("sidebar.servers"), '.global-topbar .workspace-tab-select[title="alpha"]']) {
      await click(selector);
      await click(nav("sidebar.settings"));
      expect(container.querySelector('[aria-label="settings"]')).not.toBeNull();
      await click('[aria-label="settings"] button');
      await click(nav("sidebar.servers"));
      expect(container.querySelector(".server-management")).not.toBeNull();
      await click(`.server-header-actions > button:last-child`);
      expect(container.querySelector('[aria-label="profile"]')).not.toBeNull();
      await click('[aria-label="profile"] button');
    }
    await click(nav("sidebar.servers"));
    expect(container.querySelector('[role="dialog"]')).toBeNull();
  });

  it("opens authentication in a standalone dialog from the local user menu", async () => {
    await click(nav("userMenu.open"));
    expect(document.querySelector(".user-menu-popover")?.textContent).toContain(i18n.t("userMenu.localUser"));
    await act(async () => {
      document.querySelector<HTMLButtonElement>('.user-menu-actions [role="menuitem"]')?.click();
      await import("../features/auth/AuthDialog");
    });
    expect(container.querySelector('[aria-label="auth"]')).not.toBeNull();
    expect(container.querySelector('[aria-label="settings"]')).toBeNull();
    expect(document.querySelector(".user-menu-popover")).toBeNull();
  });

  it("opens pricing from the titlebar control", async () => {
    await click('.global-topbar [aria-label="window.openPricing"]');
    await act(async () => { await import("../features/settings/PricingDialog"); });
    expect(container.querySelector('[aria-label="pricing"]')).not.toBeNull();
    await click('[aria-label="pricing"] button');
    expect(container.querySelector('[aria-label="pricing"]')).toBeNull();
  });

  it("activates and closes titlebar sessions without remounting other terminals", async () => {
    await act(async () => {
      useSessionStore.setState({ tabs: profiles.map((profile) => ({ id: `tab-${profile.id}`, profileId: profile.id, sessionId: `session-${profile.id}`, connectionAttemptId: `attempt-${profile.id}`, state: "connected", view: "terminal" })), activeTabId: "tab-alpha" });
    });
    const instances = Array.from(container.querySelectorAll('[data-testid="terminal"]'));
    expect(container.querySelectorAll(".global-topbar .workspace-tab")).toHaveLength(2);
    expect(workspace().querySelector(".workspace-tabs")).toBeNull();
    expect(container.querySelector(".workspace-tab-select[aria-current]")).toBeNull();
    await click('.global-topbar .workspace-tab-select[title="beta"]');
    expect(workspace().style.display).toBe("");
    expect(container.querySelector(".primary-rail [aria-current]")).toBeNull();
    expect(useSessionStore.getState().activeTabId).toBe("tab-beta");
    expect(terminal.mounted).toHaveBeenCalledTimes(2);
    expect(instances[1].getAttribute("data-active")).toBe("true");
    await click(nav("sidebar.servers"));
    await click('.global-topbar .workspace-tab-select[title="alpha"]');
    expect(instances[0].getAttribute("data-active")).toBe("true");
    await click(".global-topbar .workspace-tab-close");
    expect(disconnectSsh).toHaveBeenCalledWith("session-alpha");
    expect(useSessionStore.getState().activeTabId).toBe("tab-beta");
    expect(container.querySelector('[data-testid="terminal"]')).toBe(instances[1]);
    expect(terminal.disposed).toHaveBeenCalledTimes(1);
    await click(".global-topbar .workspace-tab-close");
    expect(useSessionStore.getState().tabs).toHaveLength(0);
    await click(".empty-workspace button");
    expect(container.querySelector(".server-management")).not.toBeNull();
  });

  it("searches independently without navigating or changing the management filter", async () => {
    const managementSearch = container.querySelector<HTMLInputElement>(".server-toolbar input")!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(managementSearch, "alpha");
      managementSearch.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await click(".host-select");
    await click(".server-profile-actions button");
    await click('[aria-label="connection"] button');
    const search = container.querySelector<HTMLInputElement>(".global-search input")!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(search, "missing");
      search.dispatchEvent(new Event("input", { bubbles: true }));
    });
    expect(container.querySelector(".server-management")).toBeNull();
    expect(workspace().style.display).toBe("");
    expect(document.querySelector(".global-search-popover")?.textContent).toContain(i18n.t("shell.searchNoResults"));
    await click(nav("sidebar.servers"));
    expect(container.querySelector<HTMLInputElement>(".server-toolbar input")?.value).toBe("alpha");
    expect(container.querySelectorAll(".host-select")).toHaveLength(1);
    expect(search.value).toBe("missing");
  });

  it("opens search targets through the existing connection flow and reuses connected sessions", async () => {
    const search = container.querySelector<HTMLInputElement>(".global-search input")!;
    const findAlpha = async () => {
      await act(async () => {
        search.focus();
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(search, "alpha");
        search.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await act(async () => document.querySelector<HTMLButtonElement>('.global-search-result')?.click());
    };
    await findAlpha();
    expect(workspace().style.display).toBe("");
    expect(container.querySelector('[aria-label="connection"] span')?.textContent).toBe("alpha");
    expect(connectSsh).not.toHaveBeenCalled();
    expect(document.querySelector(".global-search-popover")).toBeNull();
    await click('[aria-label="connection"] button');
    const instance = container.querySelector('[data-testid="terminal"]');
    await click(nav("sidebar.servers"));
    await findAlpha();
    expect(container.querySelector('[aria-label="connection"]')).toBeNull();
    expect(workspace().style.display).toBe("");
    expect(container.querySelector('[data-testid="terminal"]')).toBe(instance);
    expect(connectSsh).toHaveBeenCalledTimes(1);
    expect(useSessionStore.getState().tabs).toHaveLength(1);
    expect(terminal.disposed).not.toHaveBeenCalled();
  }, 15000);

  it("supports mobile navigation", async () => {
    await click(".host-select");
    await click(".server-profile-actions button");
    await click('[aria-label="connection"] button');
    await click(".global-topbar .mobile-navigation-toggle");
    expect(container.querySelector(".primary-rail.mobile-open")).not.toBeNull();
    expect(container.querySelector(nav("shell.sessions"))).toBeNull();
    await click(nav("sidebar.servers"));
    expect(container.querySelector(".mobile-navigation-backdrop")).toBeNull();
    expect(container.querySelector(".server-management")).not.toBeNull();
    await click('.global-topbar .workspace-tab-select[title="alpha"]');
    expect(workspace().style.display).toBe("");
  });
});
