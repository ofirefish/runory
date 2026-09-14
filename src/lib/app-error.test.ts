import { describe, expect, it } from "vitest";
import { appErrorCode, appErrorEndpoint } from "./app-error";

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

describe("appErrorEndpoint", () => {
  it("reads optional bastion gateway endpoint from the payload", () => {
    expect(
      appErrorEndpoint({ code: "BASTION_GATEWAY_UNREACHABLE", endpoint: "localhost:2222" }),
    ).toBe("localhost:2222");
  });

  it("reads endpoint from stringified IPC payloads", () => {
    expect(
      appErrorEndpoint('{"code":"BASTION_KOKO_UNREACHABLE","endpoint":"127.0.0.1:3022"}'),
    ).toBe("127.0.0.1:3022");
  });

  it("ignores missing or empty endpoints", () => {
    expect(appErrorEndpoint({ code: "CONNECTION_REFUSED" })).toBeUndefined();
    expect(appErrorEndpoint({ code: "BASTION_GATEWAY_UNREACHABLE", endpoint: "  " })).toBeUndefined();
  });
});
