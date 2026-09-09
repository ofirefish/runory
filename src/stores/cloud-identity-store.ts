import { create } from "zustand";

type CloudIdentityStore = {
  avatarUrl: string | null;
  setAvatarUrl: (avatarUrl: string | null) => void;
};

export const useCloudIdentityStore = create<CloudIdentityStore>((set) => ({
  avatarUrl: null,
  setAvatarUrl: (avatarUrl) => set({ avatarUrl }),
}));
