import { createClient, type SupabaseClient } from "@supabase/supabase-js";
import type { CloudDatabase } from "../../types/cloud";

const url = import.meta.env.VITE_SUPABASE_URL as string | undefined;
const publishableKey = import.meta.env.VITE_SUPABASE_PUBLISHABLE_KEY as string | undefined;

export const cloudEndpoint = url;
export const cloudPublishableKey = publishableKey;

export const cloudConfigured = Boolean(url && publishableKey);
export const supabase: SupabaseClient<CloudDatabase> | null = cloudConfigured
  ? createClient<CloudDatabase>(url!, publishableKey!, {
      auth: {
        persistSession: false,
        autoRefreshToken: true,
        detectSessionInUrl: false,
      },
    })
  : null;

/**
 * OAuth PKCE verifiers must survive the browser round trip, but must never be
 * persisted. A fresh client and in-memory store are created for each attempt.
 */
export function createCloudOAuthClient(): SupabaseClient<CloudDatabase> {
  if (!url || !publishableKey) throw new Error("CLOUD_NOT_CONFIGURED");
  const values = new Map<string, string>();
  return createClient<CloudDatabase>(url, publishableKey, {
    auth: {
      persistSession: false,
      autoRefreshToken: false,
      detectSessionInUrl: false,
      flowType: "pkce",
      storageKey: "runory-cloud-oauth",
      storage: {
        getItem: (key) => values.get(key) ?? null,
        setItem: (key, value) => { values.set(key, value); },
        removeItem: (key) => { values.delete(key); },
      },
    },
  });
}
