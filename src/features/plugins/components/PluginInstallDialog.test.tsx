// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../../i18n";
import { PluginInstallDialog } from "./PluginInstallDialog";
import type { PluginPackageInspection } from "../types/plugin";

const api = vi.hoisted(() => ({
  open: vi.fn(), inspect: vi.fn(), install: vi.fn(), discard: vi.fn(), prepare: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: api.open }));
vi.mock("../../../lib/tauri/plugins", () => ({
  pluginInspectPackage: api.inspect, pluginInstall: api.install,
  pluginDiscardInstallReview: api.discard, pluginMarketplacePrepareInstall: api.prepare,
}));

const review: PluginPackageInspection = {
  reviewId: "review-1", trust: "official", installedVersion: "1.0.0",
  permissionAdditions: ["ssh.exec"], compressedSize: 10, fileCount: 2,
  manifest: { id: "runory.test", name: "Test plugin", schemaVersion: 1, version: "1.1.0",
    publisher: { id: "runory", name: "Runory" }, apiVersion: "1",
    permissions: ["host.read", { permission: "ssh.exec", hosts: ["host-1"] }],
  },
};
let root: Root;
let container: HTMLDivElement;
const closed = vi.fn();
const installed = vi.fn();

async function click(label: string) {
  const button = [...document.querySelectorAll("button")].find((item) => item.textContent === label);
  expect(button, label).toBeTruthy();
  await act(async () => button!.click());
}
async function render(open = true) {
  await act(async () => root.render(<PluginInstallDialog open={open} onClose={closed} onInstalled={installed} />));
}
beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.clearAllMocks();
  api.open.mockResolvedValue("C:/plugins/test.runory-plugin");
  api.inspect.mockResolvedValue(review);
  api.install.mockResolvedValue({});
  api.discard.mockResolvedValue(undefined);
  container = document.createElement("div"); document.body.append(container); root = createRoot(container);
  await i18n.changeLanguage("en-US");
});
afterEach(async () => { await act(async () => root.unmount()); container.remove(); vi.unstubAllGlobals(); });

describe("plugin installation review", () => {
  it("requires confirmation, displays scope expansion and submits only selected grants", async () => {
    await render();
    await click(i18n.t("plugins.install.select"));
    expect(document.body.textContent).toContain("host-1");
    expect(document.body.textContent).toContain(i18n.t("plugins.install.expandedPermission"));
    const button = [...document.querySelectorAll("button")].find((item) => item.textContent === i18n.t("plugins.updates.confirm"))!;
    expect(button.disabled).toBe(true);
    const boxes = document.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
    await act(async () => { boxes[1]!.click(); boxes[2]!.click(); });
    await click(i18n.t("plugins.updates.confirm"));
    expect(api.install).toHaveBeenCalledExactlyOnceWith({ reviewId: "review-1", grantedPermissions: ["host.read"] });
    expect(installed).toHaveBeenCalledOnce();
    expect(closed).toHaveBeenCalledOnce();
  });

  it("invalidates confirmation when permissions change", async () => {
    await render(); await click(i18n.t("plugins.install.select"));
    const boxes = document.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
    await act(async () => boxes[2]!.click());
    await act(async () => boxes[0]!.click());
    expect(boxes[2]!.checked).toBe(false);
    expect(api.install).not.toHaveBeenCalled();
  });

  it("discards a cancelled review and reopens with no stale package", async () => {
    await render(); await click(i18n.t("plugins.install.select"));
    await render(false);
    expect(api.discard).toHaveBeenCalledWith("review-1");
    await render();
    expect(document.body.textContent).not.toContain("Test plugin");
  });

  it("clears a consumed failed review and shows the stable localized error", async () => {
    api.install.mockRejectedValue({ code: "PLUGIN_PLAN_STALE" });
    await render(); await click(i18n.t("plugins.install.select"));
    await act(async () => document.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[2]!.click());
    await click(i18n.t("plugins.updates.confirm"));
    expect(document.querySelector('[role="alert"]')?.textContent).toBe(i18n.t("errors.PLUGIN_PLAN_STALE"));
    expect(document.body.textContent).not.toContain("Test plugin");
    expect(installed).not.toHaveBeenCalled();
  });

  it("handles native picker failure without an unhandled rejection", async () => {
    api.open.mockRejectedValue({ code: "PLUGIN_PACKAGE_INVALID" });
    await render(); await click(i18n.t("plugins.install.select"));
    expect(document.querySelector('[role="alert"]')?.textContent).toBe(i18n.t("errors.PLUGIN_PACKAGE_INVALID"));
    expect(api.inspect).not.toHaveBeenCalled();
  });

  it("marketplace inspection never installs before review", async () => {
    api.prepare.mockResolvedValue(review);
    await act(async () => root.render(<PluginInstallDialog open marketplacePlugin={{
      id: "runory.test", name: "Test plugin", publisherId: "runory", latestVersion: "1.1.0", apiVersion: "1", verified: true,
    }} onClose={closed} onInstalled={installed} />));
    expect(api.prepare).not.toHaveBeenCalled();
    await click(i18n.t("plugins.install.inspectDownload"));
    expect(api.prepare).toHaveBeenCalledExactlyOnceWith("runory.test", "1.1.0");
    expect(api.install).not.toHaveBeenCalled();
  });

  it("discards an inspection that resolves after the dialog unmounts", async () => {
    let resolve!: (value: PluginPackageInspection) => void;
    api.inspect.mockReturnValue(new Promise<PluginPackageInspection>((done) => { resolve = done; }));
    await render(); await click(i18n.t("plugins.install.select"));
    await render(false);
    await act(async () => resolve(review));
    expect(api.discard).toHaveBeenCalledWith("review-1");
  });
});
