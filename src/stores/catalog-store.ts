import { create } from "zustand";
import * as catalog from "../lib/tauri/catalog";
import type { CreateGroupRequest, CreateProfileRequest, HostGroup, ServerProfile, UpdateGroupRequest, UpdateProfileRequest } from "../types/domain";

type CatalogStore = {
  groups: HostGroup[];
  profiles: ServerProfile[];
  selectedProfileId: string | null;
  loading: boolean;
  errorCode: string | null;
  selectProfile: (id: string | null) => void;
  load: () => Promise<void>;
  createGroup: (request: CreateGroupRequest) => Promise<void>;
  updateGroup: (request: UpdateGroupRequest) => Promise<void>;
  deleteGroup: (id: string) => Promise<void>;
  reorderGroups: (orderedIds: string[]) => Promise<void>;
  createProfile: (request: CreateProfileRequest) => Promise<void>;
  updateProfile: (request: UpdateProfileRequest) => Promise<void>;
  deleteProfile: (id: string) => Promise<void>;
  reorderProfiles: (groupId: string | null, orderedIds: string[]) => Promise<void>;
};

function codeOf(error: unknown): string {
  return typeof error === "object" && error !== null && "code" in error && typeof error.code === "string" ? error.code : "UNKNOWN";
}

export const useCatalogStore = create<CatalogStore>((set) => ({
  groups: [], profiles: [], selectedProfileId: null, loading: false, errorCode: null,
  selectProfile: (selectedProfileId) => set({ selectedProfileId }),
  load: async () => { set({ loading: true, errorCode: null }); try { const [groups, profiles] = await Promise.all([catalog.listGroups(), catalog.listProfiles()]); set((state) => ({ groups, profiles, selectedProfileId: profiles.some((profile) => profile.id === state.selectedProfileId) ? state.selectedProfileId : null, loading: false })); } catch (error) { set({ loading: false, errorCode: codeOf(error) }); } },
  createGroup: async (request) => { const group = await catalog.createGroup(request); set((state) => ({ groups: [...state.groups, group].sort((a, b) => a.sortOrder - b.sortOrder), errorCode: null })); },
  updateGroup: async (request) => { const group = await catalog.updateGroup(request); set((state) => ({ groups: state.groups.map((item) => item.id === group.id ? group : item).sort((a, b) => a.sortOrder - b.sortOrder), errorCode: null })); },
  deleteGroup: async (id) => { await catalog.deleteGroup(id); set((state) => ({ groups: state.groups.filter((group) => group.id !== id), profiles: state.profiles.map((profile) => profile.groupId === id ? { ...profile, groupId: null } : profile), errorCode: null })); },
  reorderGroups: async (orderedIds) => { try { const groups = await catalog.reorderGroups({ orderedIds }); set({ groups, errorCode: null }); } catch (error) { set({ errorCode: codeOf(error) }); throw error; } },
  createProfile: async (request) => { const profile = await catalog.createProfile(request); set((state) => ({ profiles: [...state.profiles, profile].sort((a, b) => a.sortOrder - b.sortOrder), errorCode: null })); },
  updateProfile: async (request) => { const profile = await catalog.updateProfile(request); set((state) => ({ profiles: state.profiles.map((item) => item.id === profile.id ? profile : item).sort((a, b) => a.sortOrder - b.sortOrder), errorCode: null })); },
  deleteProfile: async (id) => { await catalog.deleteProfile(id); set((state) => ({ profiles: state.profiles.filter((profile) => profile.id !== id), selectedProfileId: state.selectedProfileId === id ? null : state.selectedProfileId, errorCode: null })); },
  reorderProfiles: async (groupId, orderedIds) => { try { const profiles = await catalog.reorderProfiles({ groupId, orderedIds }); set({ profiles, errorCode: null }); } catch (error) { set({ errorCode: codeOf(error) }); throw error; } },
}));
