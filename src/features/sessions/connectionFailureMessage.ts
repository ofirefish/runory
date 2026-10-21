import type { TFunction } from "i18next";
import { appErrorEndpoint } from "../../lib/app-error";

const PROVIDER_SPECIFIC_CODES = new Set([
  "CONNECTION_LOST",
  "BASTION_GATEWAY_UNREACHABLE",
  "BASTION_KOKO_UNREACHABLE",
  "BASTION_UNAVAILABLE",
  "BASTION_AUTH_FAILED",
]);

/** Format optional host:port for i18n interpolation (empty when absent). */
export function formatFailureEndpoint(endpoint: string | undefined): string {
  return endpoint ? ` (${endpoint})` : "";
}

/**
 * Resolve a connection failure message.
 * JumpServer / Teleport / Boundary keep provider-specific tips;
 * direct SSH and other bastion providers use the generic copy (no KoKo jargon).
 */
export function connectionFailureMessage(
  t: TFunction,
  code: string,
  options?: { provider?: string; endpoint?: string; error?: unknown; cli?: string },
): string {
  const provider = options?.provider;
  const endpoint = formatFailureEndpoint(options?.endpoint ?? appErrorEndpoint(options?.error));
  const params = { endpoint, cli: options?.cli };
  const providerKey =
    provider && PROVIDER_SPECIFIC_CODES.has(code) ? `connection.errors.${provider}.${code}` : null;

  if (providerKey && t(providerKey, { ...params, defaultValue: "__missing__" }) !== "__missing__") {
    return t(providerKey, params);
  }

  return t(`connection.errors.${code}`, {
    ...params,
    defaultValue: t(`errors.${code}`, {
      ...params,
      defaultValue: t(provider ? "bastion.genericError" : "connection.defaultError"),
    }),
  });
}
