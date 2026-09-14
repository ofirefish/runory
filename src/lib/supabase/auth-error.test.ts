import { describe, expect, it } from "vitest";
import { cloudAuthErrorKey } from "./auth-error";

describe("cloudAuthErrorKey", () => {
  it("maps rate limits", () => {
    expect(cloudAuthErrorKey({ status: 429, message: "email rate limit exceeded" })).toBe("cloud.authRateLimited");
    expect(cloudAuthErrorKey({ code: "over_email_send_rate_limit" })).toBe("cloud.authRateLimited");
  });

  it("maps existing accounts", () => {
    expect(cloudAuthErrorKey({ code: "user_already_exists" })).toBe("cloud.authUserExists");
    expect(cloudAuthErrorKey({ message: "User already registered" })).toBe("cloud.authUserExists");
  });

  it("maps invalid credentials", () => {
    expect(cloudAuthErrorKey({ code: "invalid_credentials" })).toBe("cloud.authInvalidCredentials");
  });

  it("falls back to the generic account error", () => {
    expect(cloudAuthErrorKey(new Error("fixture"))).toBe("cloud.accountError");
    expect(cloudAuthErrorKey(new TypeError("Failed to fetch"))).toBe("cloud.accountError");
  });
});
