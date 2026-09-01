import type { AuthMethod } from "../../types/domain";
import type { CredentialInput } from "../../types/session";

export function createCredentialInput(authMethod: AuthMethod, remember: boolean, secret: string): CredentialInput | null {
  if ((authMethod === "password" || remember) && !secret) return null;
  return remember ? { mode: "remember-securely", secret } : { mode: "session-only", secret };
}
