import { Channel, invoke } from "@tauri-apps/api/core";

export type AppUpdateMetadata = {
  currentVersion: string;
  version: string;
  notes: string | null;
  date: string | null;
};

export type AppUpdateSnapshot = {
  configured: boolean;
  currentVersion: string;
  update: AppUpdateMetadata | null;
  downloaded: boolean;
};

export type AppUpdateEvent =
  | { event: "started"; totalBytes: number | null }
  | { event: "progress"; downloadedBytes: number; totalBytes: number | null }
  | { event: "finished"; downloadedBytes: number; totalBytes: number | null };

export const appUpdateStatus = () => invoke<AppUpdateSnapshot>("app_update_status");

export const checkForAppUpdate = () => invoke<AppUpdateSnapshot>("app_update_check");

export const downloadAppUpdate = (onEvent: (event: AppUpdateEvent) => void) => {
  const channel = new Channel<AppUpdateEvent>();
  channel.onmessage = onEvent;
  return invoke<AppUpdateSnapshot>("app_update_download", { onEvent: channel });
};

export const installAppUpdate = (force = false) =>
  invoke<void>("app_update_install", { request: { force } });
