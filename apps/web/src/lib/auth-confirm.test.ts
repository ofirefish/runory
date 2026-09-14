import { describe, expect, it } from "vitest";
import { resolveWebAuthConfirmStrategy } from "./auth-confirm";

describe("resolveWebAuthConfirmStrategy", () => {
  it("prefers PKCE code", () => {
    expect(resolveWebAuthConfirmStrategy("https://runory.app/auth/confirm?code=abc&token_hash=x&type=signup")).toEqual({
      kind: "code",
      code: "abc",
    });
  });

  it("uses token_hash when present", () => {
    expect(resolveWebAuthConfirmStrategy("https://runory.app/auth/confirm?token_hash=hash&type=signup")).toEqual({
      kind: "token_hash",
      tokenHash: "hash",
      type: "signup",
    });
  });

  it("reads implicit tokens from the fragment", () => {
    expect(resolveWebAuthConfirmStrategy("https://runory.app/auth/confirm#access_token=a&refresh_token=b&type=signup")).toEqual({
      kind: "fragment",
      accessToken: "a",
      refreshToken: "b",
    });
  });

  it("returns none when credentials are missing", () => {
    expect(resolveWebAuthConfirmStrategy("https://runory.app/auth/confirm").kind).toBe("none");
  });
});
