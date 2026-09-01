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
