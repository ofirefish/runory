/** Maps Supabase Auth failures to stable i18n keys. Never surfaces raw server text. */
export function cloudAuthErrorKey(error: unknown): string {
  const status = readNumber(error, "status");
  const code = readString(error, "code")?.toLowerCase() ?? "";
  const message = readString(error, "message")?.toLowerCase() ?? "";

  if (status === 429 || code.includes("rate_limit") || message.includes("rate limit") || message.includes("email rate limit")) {
    return "cloud.authRateLimited";
  }
  if (code === "user_already_exists" || code === "email_exists" || message.includes("already registered") || message.includes("already been registered")) {
    return "cloud.authUserExists";
  }
  if (code === "invalid_credentials" || message.includes("invalid login credentials")) {
    return "cloud.authInvalidCredentials";
  }
  if (error instanceof TypeError || message.includes("failed to fetch") || message.includes("networkerror") || message.includes("load failed")) {
    return "cloud.accountError";
  }
  return "cloud.accountError";
}

function readNumber(error: unknown, key: string): number | undefined {
  if (!error || typeof error !== "object" || !(key in error)) return undefined;
  const value = (error as Record<string, unknown>)[key];
  return typeof value === "number" ? value : undefined;
}

function readString(error: unknown, key: string): string | undefined {
  if (!error || typeof error !== "object" || !(key in error)) return undefined;
  const value = (error as Record<string, unknown>)[key];
  return typeof value === "string" ? value : undefined;
}
