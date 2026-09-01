import { describe, expect, it } from "vitest";
import { connectionOutcome } from "./connection-outcome";

describe("connectionOutcome", () => {
  it("closes after a successful session-only connection", () => {
    expect(connectionOutcome("connect", "session-only", false)).toEqual({
      closeDialog: true,
      saveWarning: false,
      testSucceeded: false,
    });
  });

  it("closes after connecting with an existing stored credential", () => {
    expect(connectionOutcome("connect", "stored", false).closeDialog).toBe(true);
  });

  it("keeps the warning visible only when a requested save failed", () => {
    expect(connectionOutcome("connect", "remember-securely", false)).toEqual({
      closeDialog: false,
      saveWarning: true,
      testSucceeded: false,
    });
  });

  it("reports a clean test success when saving was not requested", () => {
    expect(connectionOutcome("test", "session-only", false)).toEqual({
      closeDialog: false,
      saveWarning: false,
      testSucceeded: true,
    });
  });
});
