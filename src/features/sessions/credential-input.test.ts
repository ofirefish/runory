import { describe, expect, it } from "vitest";
import { createCredentialInput } from "./credential-input";

describe("connection credential input", () => {
  it("requires a password but permits an empty passphrase for an unencrypted key", () => {
    expect(createCredentialInput("password", false, "")).toBeNull();
    expect(createCredentialInput("privateKey", false, "")).toEqual({ mode: "session-only", secret: "" });
  });

  it("only requests persistence when the user explicitly enables it", () => {
    expect(createCredentialInput("password", false, "secret")).toEqual({ mode: "session-only", secret: "secret" });
    expect(createCredentialInput("privateKey", true, "phrase")).toEqual({ mode: "remember-securely", secret: "phrase" });
    expect(createCredentialInput("privateKey", true, "")).toBeNull();
  });
});
