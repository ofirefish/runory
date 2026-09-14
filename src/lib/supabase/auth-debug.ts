const PREFIX = "[runory:auth]";

/** Debug-only auth traces. Never pass passwords, tokens, or raw deep-link URLs with secrets. */
export function logAuthDebug(step: string, details?: Record<string, unknown>): void {
  if (details) console.info(PREFIX, step, details);
  else console.info(PREFIX, step);
}

export function authErrorDebugInfo(error: unknown): Record<string, unknown> {
  if (!error || typeof error !== "object") {
    return { kind: typeof error, message: error instanceof Error ? error.message : String(error) };
  }
  const record = error as Record<string, unknown>;
  return {
    name: typeof record.name === "string" ? record.name : undefined,
    status: typeof record.status === "number" ? record.status : undefined,
    code: typeof record.code === "string" ? record.code : undefined,
    message: typeof record.message === "string" ? record.message : undefined,
  };
}

/** Safe summary of a deep link: never includes access/refresh tokens. */
export function authDeepLinkDebugInfo(value: string): Record<string, unknown> {
  try {
    const url = new URL(value);
    const fragment = new URLSearchParams(url.hash.startsWith("#") ? url.hash.slice(1) : url.hash);
    return {
      protocol: url.protocol,
      host: url.hostname,
      path: url.pathname,
      hasCode: url.searchParams.has("code"),
      hasQueryTokens: url.searchParams.has("access_token") || url.searchParams.has("refresh_token"),
      hasFragmentTokens: fragment.has("access_token") && fragment.has("refresh_token"),
      fragmentType: fragment.get("type"),
      queryError: url.searchParams.get("error"),
      fragmentError: fragment.get("error"),
    };
  } catch {
    return { malformed: true };
  }
}
