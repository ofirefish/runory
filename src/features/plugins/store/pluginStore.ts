import { create } from "zustand";
import type { PluginDto, PluginHostMenuDto } from "../types/plugin";
import {
  pluginHostMenus,
  pluginListInstalled,
  pluginSetEnabled,
  pluginUninstall,
} from "../../../lib/tauri/plugins";

type PluginStore = {
  plugins: PluginDto[];
  hostMenus: PluginHostMenuDto[];
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  refreshHostMenus: (hostOs?: string | null, hostArch?: string | null) => Promise<void>;
  setEnabled: (pluginId: string, enabled: boolean) => Promise<void>;
  uninstall: (pluginId: string) => Promise<void>;
};

let listRequest = 0;
let menuRequest = 0;
let menuContext: [string | null | undefined, string | null | undefined] = [undefined, undefined];

export const usePluginStore = create<PluginStore>((set, get) => ({
  plugins: [],
  hostMenus: [],
  loading: false,
  error: null,
  refresh: async () => {
    const request = ++listRequest;
    set({ loading: true, error: null });
    try {
      const plugins = await pluginListInstalled();
      if (request !== listRequest) return;
      set({ plugins, loading: false });
      await get().refreshHostMenus(...menuContext);
    } catch {
      if (request === listRequest) set({ loading: false, error: "loadFailed" });
    }
  },
  refreshHostMenus: async (hostOs, hostArch) => {
    menuContext = [hostOs, hostArch];
    const request = ++menuRequest;
    try {
      const hostMenus = await pluginHostMenus(hostOs, hostArch);
      if (request === menuRequest) set({ hostMenus });
    } catch {
      if (request === menuRequest) set({ hostMenus: [] });
    }
  },
  setEnabled: async (pluginId, enabled) => {
    await pluginSetEnabled(pluginId, enabled);
    menuRequest += 1;
    set({ hostMenus: get().hostMenus.filter((menu) => menu.pluginId !== pluginId) });
    await get().refresh();
  },
  uninstall: async (pluginId) => {
    await pluginUninstall(pluginId);
    menuRequest += 1;
    set({ hostMenus: get().hostMenus.filter((menu) => menu.pluginId !== pluginId) });
    await get().refresh();
  },
}));
