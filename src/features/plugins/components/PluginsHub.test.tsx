// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../../i18n";
import { PluginsHub } from "./PluginsHub";
import { usePluginStore } from "../store/pluginStore";
import type { MarketplacePlugin, PluginDto } from "../types/plugin";

const api = vi.hoisted(() => ({ catalog: vi.fn(), revoke: vi.fn(), settings: vi.fn(), updateSettings: vi.fn(), refresh: vi.fn(), enable: vi.fn(), uninstall: vi.fn() }));
vi.mock("../../../lib/tauri/plugins", () => ({ pluginMarketplaceList: api.catalog, pluginMarketplaceSyncRevocations: api.revoke }));
vi.mock("../../../lib/tauri/settings", () => ({ getSettings: api.settings, updateSettings: api.updateSettings }));
vi.mock("./PluginInstallDialog", () => ({ PluginInstallDialog: ({ open, marketplacePlugin }: { open: boolean; marketplacePlugin?: MarketplacePlugin }) => open ? <div data-testid="review">{marketplacePlugin?.latestVersion}</div> : null }));

const installed: PluginDto = { id: "test.plugin", name: "Test Plugin", version: "1.0.0", publisherId: "test", enabled: true, description: "Installed description", permissions: [], commands: [], workflows: [], hostMenus: [] };
const candidate: MarketplacePlugin = { id: installed.id, name: installed.name, publisherId: "test", latestVersion: "1.10.0", installedVersion: "1.0.0", updateAvailable: true, apiVersion: "1", verified: false };
let root: Root;
let container: HTMLDivElement;
beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.clearAllMocks();
  api.catalog.mockResolvedValue([candidate]); api.revoke.mockResolvedValue(0); api.settings.mockResolvedValue({ pluginDeveloperMode: false }); api.refresh.mockResolvedValue(undefined);
  usePluginStore.setState({ plugins: [installed], loading: false, error: null, refresh: api.refresh, setEnabled: api.enable, uninstall: api.uninstall });
  container = document.createElement("div"); document.body.append(container); root = createRoot(container);
  await i18n.changeLanguage("en-US");
});
afterEach(async () => { await act(async () => root.unmount()); container.remove(); vi.unstubAllGlobals(); });

describe("plugin hub", () => {
  it("shows real updates and opens the package review", async () => {
    await act(async () => root.render(<PluginsHub initialTab="updates" />));
    const updates = container.querySelector('[data-section="updates"]')!;
    expect(updates.textContent).toContain("1.0.0 → 1.10.0");
    expect(updates.querySelector('.plugins-hub-group-count')?.textContent).toBe("1");
    const action = [...updates.querySelectorAll("button")].find((button) => button.textContent === i18n.t("plugins.updates.review"))!;
    await act(async () => action.click());
    expect(container.querySelector('[data-testid="review"]')?.textContent).toBe("1.10.0");
  });
  it("shows marketplace failure instead of a false no-updates result, and offers refresh", async () => {
    api.catalog.mockRejectedValue(new Error("offline"));
    await act(async () => root.render(<PluginsHub initialTab="updates" />));
    const updates = container.querySelector('[data-section="updates"]')!;
    expect(updates.textContent).toContain(i18n.t("plugins.marketplace.unavailable"));
    expect(updates.textContent).not.toContain(i18n.t("plugins.updates.empty"));
    api.catalog.mockResolvedValue([candidate]);
    const refresh = [...container.querySelectorAll("button")].find((button) => button.textContent === i18n.t("plugins.action.refresh"))!;
    await act(async () => refresh.click());
    expect(updates.textContent).toContain("1.0.0 → 1.10.0");
  });
  it("localizes lifecycle failure and preserves installed details", async () => {
    api.enable.mockRejectedValue({ code: "PLUGIN_REVOKED" });
    await act(async () => root.render(<PluginsHub />));
    const disable = [...container.querySelectorAll("button")].find((button) => button.textContent === i18n.t("plugins.action.disable"))!;
    await act(async () => disable.click());
    expect(container.querySelector('[role="alert"]')?.textContent).toBe(i18n.t("errors.PLUGIN_REVOKED"));
    expect(container.textContent).toContain("Installed description");
  });
  it("requires a concrete confirmation before uninstalling", async () => {
    api.uninstall.mockResolvedValue(undefined);
    await act(async () => root.render(<PluginsHub />));
    const uninstall = [...container.querySelectorAll("button")].find((button) => button.textContent === i18n.t("plugins.action.uninstall"))!;
    await act(async () => uninstall.click());
    expect(api.uninstall).not.toHaveBeenCalled();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain(installed.name);
    const confirm = [...dialog.querySelectorAll("button")].find((button) => button.textContent === i18n.t("plugins.action.uninstall"))!;
    await act(async () => confirm.click());
    expect(api.uninstall).toHaveBeenCalledExactlyOnceWith(installed.id);
  });
});
