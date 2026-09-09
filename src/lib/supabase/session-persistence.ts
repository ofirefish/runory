import { invoke, isTauri } from "@tauri-apps/api/core";
import type { AuthChangeEvent, Session } from "@supabase/supabase-js";
import { supabase } from "./client";

type StoredCloudSession = {
  accessToken: string;
  refreshToken: string;
};

export const cloudSessionPersistenceChangedEvent = "runory:cloud-session-persistence-changed";
let persistenceFailed = false;
let initialized = false;

function reportPersistenceFailure() {
  persistenceFailed = true;
  window.dispatchEvent(new Event(cloudSessionPersistenceChangedEvent));
}

function reportPersistenceSuccess() {
  persistenceFailed = false;
  window.dispatchEvent(new Event(cloudSessionPersistenceChangedEvent));
}

export function cloudSessionPersistenceFailed(): boolean {
  return persistenceFailed;
}

export async function persistCloudSession(session: Session): Promise<boolean> {
  if (!isTauri()) return false;
  try {
    await invoke("cloud_auth_session_save", {
      session: { accessToken: session.access_token, refreshToken: session.refresh_token },
    });
    reportPersistenceSuccess();
    return true;
  } catch {
    reportPersistenceFailure();
    return false;
  }
}

export async function clearPersistedCloudSession(): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke("cloud_auth_session_clear");
    reportPersistenceSuccess();
  } catch {
    reportPersistenceFailure();
  }
}

function persistAuthChange(event: AuthChangeEvent, session: Session | null) {
  if (session && (event === "SIGNED_IN" || event === "TOKEN_REFRESHED")) {
    void persistCloudSession(session);
  } else if (event === "SIGNED_OUT") {
    void clearPersistedCloudSession();
  }
}

export async function initializeCloudSessionPersistence(): Promise<void> {
  if (initialized || !isTauri() || !supabase) return;
  initialized = true;
  supabase.auth.onAuthStateChange(persistAuthChange);
  try {
    const stored = await invoke<StoredCloudSession | null>("cloud_auth_session_load");
    if (!stored) return;
    const { data, error } = await supabase.auth.setSession({
      access_token: stored.accessToken,
      refresh_token: stored.refreshToken,
    });
    if (error || !data.session) {
      await clearPersistedCloudSession();
      reportPersistenceFailure();
    }
  } catch {
    reportPersistenceFailure();
  }
}
