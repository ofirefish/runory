import { describe, expect, it } from "vitest";
import { authDeepLinkDebugInfo, authErrorDebugInfo } from "./auth-debug";

describe("auth debug helpers", () => {
  it("summarizes deep links without exposing tokens", () => {
    const info = authDeepLinkDebugInfo("runory://auth/callback#access_token=secret&refresh_token=secret&type=email_confirm");
    expect(info).toMatchObject({
      hasFragmentTokens: true,
      fragmentType: "email_confirm",
      hasCode: false,
      hasQueryTokens: false,
    });
    expect(JSON.stringify(info)).not.toContain("secret");
  });

  it("extracts Auth error fields safely", () => {
    expect(authErrorDebugInfo({ status: 429, code: "over_email_send_rate_limit", message: "rate limit" })).toEqual({
      name: undefined,
      status: 429,
      code: "over_email_send_rate_limit",
      message: "rate limit",
    });
  });
});
