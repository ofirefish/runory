import type { CredentialInput } from "../../types/session";

export type ConnectionAction = "connect" | "test";

export type ConnectionOutcome = {
  closeDialog: boolean;
  saveWarning: boolean;
  testSucceeded: boolean;
};

export function connectionOutcome(
  action: ConnectionAction,
  credentialMode: CredentialInput["mode"],
  credentialSaved: boolean,
): ConnectionOutcome {
  const saveWasRequested = credentialMode === "remember-securely";
  const saveWarning = saveWasRequested && !credentialSaved;
  return {
    closeDialog: action === "connect" && !saveWarning,
    saveWarning,
    testSucceeded: action === "test",
  };
}
