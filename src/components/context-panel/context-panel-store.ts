import { create } from "zustand";

/**
 * Right Context Panel UI preferences (Inspector / Agent). Local state only —
 * no LLM context, no credentials, no orchestration.
 */
type ContextPanelStore = {
  visible: boolean;
  collapsed: boolean;
  activeTab: "inspector" | "agent";
  setVisible: (visible: boolean) => void;
  setCollapsed: (collapsed: boolean) => void;
  setActiveTab: (tab: "inspector" | "agent") => void;
};

const VISIBLE_KEY = "runory.contextPanelVisible";
const TAB_KEY = "runory.contextTab";

function read(key: string, fallback: string): string {
  try { return localStorage.getItem(key) ?? fallback; } catch { return fallback; }
}

export const useContextPanelStore = create<ContextPanelStore>((set) => ({
  visible: read(VISIBLE_KEY, "true") === "true",
  collapsed: false,
  activeTab: read(TAB_KEY, "inspector") === "agent" ? "agent" : "inspector",
  setVisible: (visible) => {
    try { localStorage.setItem(VISIBLE_KEY, String(visible)); } catch { /* ignore */ }
    set({ visible });
  },
  setCollapsed: (collapsed) => set({ collapsed }),
  setActiveTab: (tab) => {
    try { localStorage.setItem(TAB_KEY, tab); } catch { /* ignore */ }
    set({ activeTab: tab });
  },
}));