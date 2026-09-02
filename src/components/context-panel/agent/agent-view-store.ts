import { create } from "zustand";

type AgentViewStore = {
  expanded: Record<string, boolean>;
  approachExpanded: boolean;
  setExpanded: (key: string, value: boolean) => void;
  toggleExpanded: (key: string) => void;
  setApproachExpanded: (value: boolean) => void;
};

export const useAgentViewStore = create<AgentViewStore>((set) => ({
  expanded: {},
  approachExpanded: true,
  setExpanded: (key, value) => set((state) => ({ expanded: { ...state.expanded, [key]: value } })),
  toggleExpanded: (key) => set((state) => ({ expanded: { ...state.expanded, [key]: !state.expanded[key] } })),
  setApproachExpanded: (value) => set({ approachExpanded: value }),
}));
