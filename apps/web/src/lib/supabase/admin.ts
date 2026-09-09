import "server-only";
import { createClient } from "@supabase/supabase-js";
import { getSupabasePublicConfig } from "./config";

export function createSupabaseAdminClient() {
  const config = getSupabasePublicConfig();
  const secretKey = process.env.SUPABASE_SECRET_KEY;
  if (!config || !secretKey) return null;
  return createClient(config.url, secretKey, {
    auth: { autoRefreshToken: false, persistSession: false, detectSessionInUrl: false },
  });
}

