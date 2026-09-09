import { create } from "zustand";
import { appErrorCode } from "../lib/app-error";
import {
  appUpdateStatus,
  checkForAppUpdate,
  downloadAppUpdate,
  installAppUpdate,
  type AppUpdateMetadata,
} from "../lib/tauri/app-update";

export type AppUpdatePhase =
  | "unsupported"
  | "idle"
  | "checking"
  | "up-to-date"
  | "available"
  | "downloading"
  | "ready"
  | "installing"
  | "busy"
  | "error";

type AppUpdateStore = {
  phase: AppUpdatePhase;
  configured: boolean;
  currentVersion: string | null;
  update: AppUpdateMetadata | null;
  downloadedBytes: number;
  totalBytes: number | null;
  errorCode: string | null;
  initialize: () => Promise<void>;
  check: (automatic?: boolean) => Promise<void>;
  download: () => Promise<void>;
  install: (force?: boolean) => Promise<void>;
};

export const useAppUpdateStore = create<AppUpdateStore>((set, get) => ({
  phase: "idle",
  configured: false,
  currentVersion: null,
  update: null,
  downloadedBytes: 0,
  totalBytes: null,
  errorCode: null,
  initialize: async () => {
    if (get().phase !== "idle") return;
    try {
      const snapshot = await appUpdateStatus();
      if (!snapshot.configured) {
        set({ phase: "unsupported", configured: false, currentVersion: snapshot.currentVersion });
        return;
      }
      set({
        configured: true,
        currentVersion: snapshot.currentVersion,
        update: snapshot.update,
        phase: snapshot.downloaded ? "ready" : snapshot.update ? "available" : "idle",
      });
      await get().check(true);
    } catch {
      // The commands do not exist in mobile and browser builds by design.
      set({ phase: "unsupported", configured: false });
    }
  },
  check: async (automatic = false) => {
    if (get().phase === "checking" || get().phase === "downloading" || get().phase === "installing") return;
    set({ phase: "checking", errorCode: null, downloadedBytes: 0, totalBytes: null });
    try {
      const snapshot = await checkForAppUpdate();
      set({ configured: snapshot.configured, currentVersion: snapshot.currentVersion, update: snapshot.update });
      if (!snapshot.update) {
        set({ phase: "up-to-date" });
        return;
      }
      set({ phase: "available" });
      if (automatic) await get().download();
    } catch (error) {
      const code = appErrorCode(error);
      if (automatic && code === "UPDATE_NOT_CONFIGURED") {
        set({ phase: "unsupported", configured: false, errorCode: null });
      } else {
        set({ phase: "error", errorCode: code });
      }
    }
  },
  download: async () => {
    if (!get().update || get().phase === "downloading") return;
    set({ phase: "downloading", downloadedBytes: 0, totalBytes: null, errorCode: null });
    try {
      const snapshot = await downloadAppUpdate((event) => {
        if (event.event === "started") {
          set({ totalBytes: event.totalBytes });
        } else {
          set({ downloadedBytes: event.downloadedBytes, totalBytes: event.totalBytes });
        }
      });
      set({ phase: "ready", update: snapshot.update });
    } catch (error) {
      set({ phase: "error", errorCode: appErrorCode(error) });
    }
  },
  install: async (force = false) => {
    if (!get().update || (get().phase !== "ready" && get().phase !== "busy")) return;
    set({ phase: "installing", errorCode: null });
    try {
      await installAppUpdate(force);
    } catch (error) {
      const code = appErrorCode(error);
      set({ phase: code === "UPDATE_BUSY" ? "busy" : "error", errorCode: code });
    }
  },
}));
