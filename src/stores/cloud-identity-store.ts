import { create } from "zustand";

type CloudIdentityStore = {
  avatarUrl: string | null;
  displayName: string | null;
  setAvatarUrl: (avatarUrl: string | null) => void;
  setIdentity: (identity: { avatarUrl: string | null; displayName: string | null }) => void;
};

export const useCloudIdentityStore = create<CloudIdentityStore>((set) => ({
  avatarUrl: null,
  displayName: null,
  setAvatarUrl: (avatarUrl) => set({ avatarUrl }),
  setIdentity: (identity) => set(identity),
}));
