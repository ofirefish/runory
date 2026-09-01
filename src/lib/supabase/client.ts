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
