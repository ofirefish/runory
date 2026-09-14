const APP_ERROR_CODE = /^[A-Z][A-Z0-9_]{1,127}$/;

/**
 * Tauri normally rejects commands with the serialized AppError payload, but
 * bridge and mock layers may surface the same payload as a JSON string/Error.
 * Keep the UI on stable error codes without exposing backend error text.
 */
export function appErrorCode(error: unknown): string {
  if (typeof error === "object" && error !== null) {
    if ("code" in error && typeof error.code === "string") return normalizedCode(error.code);
    if (error instanceof Error) return codeFromString(error.message);
  }
  return typeof error === "string" ? codeFromString(error) : "UNKNOWN";
}

/** Optional content-free `host:port` from bastion gateway probe failures. */
export function appErrorEndpoint(error: unknown): string | undefined {
  const payload = errorPayload(error);
  if (!payload || typeof payload.endpoint !== "string") return undefined;
  const endpoint = payload.endpoint.trim();
  return endpoint.length > 0 && endpoint.length <= 253 ? endpoint : undefined;
}

function errorPayload(error: unknown): Record<string, unknown> | null {
  if (typeof error === "object" && error !== null) {
    return error as Record<string, unknown>;
  }
  if (typeof error === "string" || (error instanceof Error && typeof error.message === "string")) {
    const raw = typeof error === "string" ? error : error.message;
    try {
      const parsed: unknown = JSON.parse(raw.trim());
      if (typeof parsed === "object" && parsed !== null) return parsed as Record<string, unknown>;
    } catch {
      // Non-JSON IPC errors have no structured payload.
    }
  }
  return null;
}

function codeFromString(value: string): string {
  const trimmed = value.trim();
  if (APP_ERROR_CODE.test(trimmed)) return trimmed;
  try {
    const parsed: unknown = JSON.parse(trimmed);
    if (parsed !== value) return appErrorCode(parsed);
  } catch {
    // Non-JSON IPC errors intentionally remain UNKNOWN.
  }
  return "UNKNOWN";
}

function normalizedCode(value: string): string {
  const trimmed = value.trim();
  return APP_ERROR_CODE.test(trimmed) ? trimmed : "UNKNOWN";
}
