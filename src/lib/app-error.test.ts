import { describe, expect, it } from "vitest";
import { appErrorCode } from "./app-error";

describe("appErrorCode", () => {
  it("reads the structured Tauri error payload", () => {
    expect(appErrorCode({ code: "MODEL_AUTH_FAILED" })).toBe("MODEL_AUTH_FAILED");
  });

  it("reads payloads stringified by an IPC bridge", () => {
    expect(appErrorCode('{"code":"VAULT_LOCKED"}')).toBe("VAULT_LOCKED");
    expect(appErrorCode(new Error('{"code":"MODEL_UNAVAILABLE"}'))).toBe("MODEL_UNAVAILABLE");
  });

  it("does not expose arbitrary backend error text as a translation key", () => {
    expect(appErrorCode("request failed with secret detail")).toBe("UNKNOWN");
    expect(appErrorCode({ code: "not a stable code" })).toBe("UNKNOWN");
  });
});
