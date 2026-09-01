import { create } from "zustand";
import type { SessionState } from "../types/session";

export type SessionTab = {
  id: string;
  profileId: string;
  title?: string;
  sessionId: string | null;
  connectionAttemptId: string;
  state: SessionState;
  view: "terminal" | "assistant" | "agent" | "files" | "dashboard" | "operations" | "deployment" | "details";
};

type SessionStore = {
  tabs: SessionTab[];
  activeTabId: string | null;
  addTab: (tab: SessionTab) => void;
  beginReconnect: (tabId: string, connectionAttemptId: string) => void;
  attachSession: (tabId: string, connectionAttemptId: string, sessionId: string) => void;
  setState: (tabId: string, connectionAttemptId: string, state: SessionState) => void;
  markClosed: (tabId: string, connectionAttemptId: string) => void;
  markError: (tabId: string, connectionAttemptId: string) => void;
  setActive: (tabId: string) => void;
  setView: (tabId: string, view: SessionTab["view"]) => void;
  renameTab: (tabId: string, title: string) => void;
  removeTab: (tabId: string) => void;
};

export const useSessionStore = create<SessionStore>((set) => ({
  tabs: [],
  activeTabId: null,
  addTab: (tab) => set(({ tabs }) => ({ tabs: [...tabs, tab], activeTabId: tab.id })),
  beginReconnect: (tabId, connectionAttemptId) => set(({ tabs }) => ({
    tabs: tabs.map((tab) => tab.id === tabId
      ? { ...tab, sessionId: null, connectionAttemptId, state: "connecting" }
      : tab),
    activeTabId: tabId,
  })),
  attachSession: (tabId, connectionAttemptId, sessionId) => set(({ tabs }) => ({
    tabs: tabs.map((tab) => tab.id === tabId && tab.connectionAttemptId === connectionAttemptId
      ? { ...tab, sessionId, state: "connected" }
      : tab),
  })),
  setState: (tabId, connectionAttemptId, state) => set(({ tabs }) => ({
    tabs: tabs.map((tab) => tab.id === tabId && tab.connectionAttemptId === connectionAttemptId
      ? { ...tab, state }
      : tab),
  })),
  markClosed: (tabId, connectionAttemptId) => set(({ tabs }) => ({
    tabs: tabs.map((tab) => tab.id === tabId && tab.connectionAttemptId === connectionAttemptId
      ? { ...tab, sessionId: null, state: "disconnected" }
      : tab),
  })),
  markError: (tabId, connectionAttemptId) => set(({ tabs }) => ({
    tabs: tabs.map((tab) => tab.id === tabId && tab.connectionAttemptId === connectionAttemptId
      ? { ...tab, sessionId: null, state: "error" }
      : tab),
  })),
  setActive: (tabId) => set(({ tabs, activeTabId }) => ({
    activeTabId: tabs.some((tab) => tab.id === tabId) ? tabId : activeTabId,
  })),
  setView: (tabId, view) => set(({ tabs }) => ({
    tabs: tabs.map((tab) => tab.id === tabId ? { ...tab, view } : tab),
  })),
  renameTab: (tabId, title) => set(({ tabs }) => ({
    tabs: tabs.map((tab) => tab.id === tabId ? { ...tab, title } : tab),
  })),
  removeTab: (tabId) => set(({ tabs, activeTabId }) => {
    const index = tabs.findIndex((tab) => tab.id === tabId);
    if (index < 0) return { tabs, activeTabId };
    const remaining = tabs.filter((tab) => tab.id !== tabId);
    const nextActive = activeTabId === tabId
      ? remaining[Math.min(index, remaining.length - 1)]?.id ?? null
      : activeTabId;
    return { tabs: remaining, activeTabId: nextActive };
  }),
}));
