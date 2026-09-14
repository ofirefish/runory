import { describe, expect, it } from "vitest";
import { buildDesktopEmailConfirmDeepLink } from "./desktop-deep-link";

describe("buildDesktopEmailConfirmDeepLink", () => {
  it("puts tokens only in the fragment", () => {
    const link = buildDesktopEmailConfirmDeepLink("access-token", "refresh-token");
    const url = new URL(link);
    expect(url.protocol).toBe("runory:");
    expect(url.hostname).toBe("auth");
    expect(url.pathname).toBe("/callback");
    expect(url.search).toBe("");
    const hash = new URLSearchParams(url.hash.slice(1));
    expect(hash.get("access_token")).toBe("access-token");
    expect(hash.get("refresh_token")).toBe("refresh-token");
    expect(hash.get("type")).toBe("email_confirm");
  });
});
