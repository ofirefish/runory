export type SupabasePublicConfig = { url: string; publishableKey: string };

export function getSupabasePublicConfig(): SupabasePublicConfig | null {
  const url = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const publishableKey = process.env.NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY;
  return url && publishableKey ? { url, publishableKey } : null;
}

export function getRunoryOrigin(): string | null {
  const value = process.env.RUNORY_APP_ORIGIN;
  if (!value) return null;
  try {
    const origin = new URL(value);
    return origin.origin === value.replace(/\/$/, "") ? origin.origin : null;
  } catch {
    return null;
  }
}

