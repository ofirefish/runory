import type { EmailOtpType, Session, SupabaseClient } from "@supabase/supabase-js";

const otpTypes = new Set<string>(["signup", "invite", "magiclink", "recovery", "email_change", "email"]);

export type WebAuthConfirmStrategy =
  | { kind: "code"; code: string }
  | { kind: "token_hash"; tokenHash: string; type: EmailOtpType }
  | { kind: "fragment"; accessToken: string; refreshToken: string }
  | { kind: "none" };

export function resolveWebAuthConfirmStrategy(href: string): WebAuthConfirmStrategy {
  const url = new URL(href);
  const fragment = new URLSearchParams(url.hash.startsWith("#") ? url.hash.slice(1) : url.hash);
  const code = url.searchParams.get("code");
  const tokenHash = url.searchParams.get("token_hash");
  const type = url.searchParams.get("type");
  if (code) return { kind: "code", code };
  if (tokenHash && type && otpTypes.has(type)) return { kind: "token_hash", tokenHash, type: type as EmailOtpType };
  const accessToken = fragment.get("access_token");
  const refreshToken = fragment.get("refresh_token");
  if (accessToken && refreshToken) return { kind: "fragment", accessToken, refreshToken };
  return { kind: "none" };
}

export async function completeWebAuthConfirm(
  client: SupabaseClient,
  href: string,
): Promise<{ session: Session; strategy: Exclude<WebAuthConfirmStrategy["kind"], "none"> }> {
  const strategy = resolveWebAuthConfirmStrategy(href);
  if (strategy.kind === "none") throw new Error("AUTH_CONFIRM_CREDENTIALS_MISSING");

  if (strategy.kind === "code") {
    const { data, error } = await client.auth.exchangeCodeForSession(strategy.code);
    if (error || !data.session) throw error ?? new Error("AUTH_CONFIRM_SESSION_MISSING");
    return { session: data.session, strategy: "code" };
  }

  if (strategy.kind === "token_hash") {
    const { data, error } = await client.auth.verifyOtp({
      type: strategy.type,
      token_hash: strategy.tokenHash,
    });
    if (error || !data.session) throw error ?? new Error("AUTH_CONFIRM_SESSION_MISSING");
    return { session: data.session, strategy: "token_hash" };
  }

  const { data, error } = await client.auth.setSession({
    access_token: strategy.accessToken,
    refresh_token: strategy.refreshToken,
  });
  if (error || !data.session) throw error ?? new Error("AUTH_CONFIRM_SESSION_MISSING");
  return { session: data.session, strategy: "fragment" };
}
