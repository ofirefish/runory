import { beforeEach, describe, expect, it, vi } from "vitest";
import { usePluginStore } from "./pluginStore";
import type { PluginDto } from "../types/plugin";

const api = vi.hoisted(() => ({ list: vi.fn(), menus: vi.fn(), enable: vi.fn(), uninstall: vi.fn() }));
vi.mock("../../../lib/tauri/plugins", () => ({ pluginListInstalled: api.list, pluginHostMenus: api.menus, pluginSetEnabled: api.enable, pluginUninstall: api.uninstall }));
const plugin: PluginDto = { id: "test.plugin", version: "1.0.0", name: "Test", publisherId: "test", enabled: true, permissions: [], commands: [], workflows: [], hostMenus: [] };

beforeEach(() => {
  vi.clearAllMocks();
  usePluginStore.setState({ plugins: [], hostMenus: [], error: null, loading: false });
  api.menus.mockResolvedValue([]); api.list.mockResolvedValue([]); api.enable.mockResolvedValue(undefined); api.uninstall.mockResolvedValue(undefined);
});

describe("plugin lifecycle store", () => {
  it("ignores older installed-list responses", async () => {
    let first!: (plugins: PluginDto[]) => void;
    api.list.mockReturnValueOnce(new Promise<PluginDto[]>((resolve) => { first = resolve; })).mockResolvedValueOnce([{ ...plugin, version: "2.0.0" }]);
    const earlier = usePluginStore.getState().refresh();
    await usePluginStore.getState().refresh(); first([plugin]); await earlier;
    expect(usePluginStore.getState().plugins[0]?.version).toBe("2.0.0");
  });
  it("refreshes host menus after lifecycle changes with the last host context", async () => {
    await usePluginStore.getState().refreshHostMenus("linux", "aarch64");
    await usePluginStore.getState().setEnabled(plugin.id, false);
    expect(api.menus).toHaveBeenLastCalledWith("linux", "aarch64");
    expect(api.menus).toHaveBeenCalledTimes(2);
  });
  it("propagates mutation failures for UI feedback without changing local state", async () => {
    usePluginStore.setState({ plugins: [plugin] });
    api.uninstall.mockRejectedValue({ code: "STORAGE_ERROR" });
    await expect(usePluginStore.getState().uninstall(plugin.id)).rejects.toEqual({ code: "STORAGE_ERROR" });
    expect(usePluginStore.getState().plugins).toEqual([plugin]);
  });
});
