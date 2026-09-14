import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  exchangeCodeForSession: vi.fn(),
  setSession: vi.fn(),
  signInWithOAuth: vi.fn(),
  openUrl: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: mocks.openUrl }));
vi.mock("./client", () => ({
  cloudEndpoint: "https://fixture.supabase.co",
  createCloudOAuthClient: () => ({ auth: {
    exchangeCodeForSession: mocks.exchangeCodeForSession,
    signInWithOAuth: mocks.signInWithOAuth,
  } }),
  supabase: { auth: { setSession: mocks.setSession } },
}));

import {
  buildCloudEmailConfirmDeepLink,
  cloudSignInWithGitHub,
  cloudSignInWithGoogle,
  completeCloudAuthDeepLink,
  completeCloudOAuthRedirect,
  isCloudOAuthRedirect,
} from "./cloud";

const session = {
  access_token: "access-token",
  refresh_token: "refresh-token",
  expires_in: 3600,
  token_type: "bearer",
  user: { id: "user-id" },
};

describe("cloud social OAuth", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.signInWithOAuth.mockResolvedValue({
      data: { provider: "google", url: "https://fixture.supabase.co/auth/v1/authorize?provider=google" },
      error: null,
    });
    mocks.exchangeCodeForSession.mockResolvedValue({ data: { session }, error: null });
    mocks.setSession.mockResolvedValue({ data: { session, user: session.user }, error: null });
  });

  it("opens only the configured Supabase Google authorization endpoint", async () => {
    await cloudSignInWithGoogle();
    expect(mocks.signInWithOAuth).toHaveBeenCalledWith({
      provider: "google",
      options: { redirectTo: "runory://auth/callback", skipBrowserRedirect: true },
    });
    expect(mocks.openUrl).toHaveBeenCalledExactlyOnceWith("https://fixture.supabase.co/auth/v1/authorize?provider=google");
  });

  it("opens only the configured Supabase GitHub authorization endpoint", async () => {
    mocks.signInWithOAuth.mockResolvedValueOnce({
      data: { provider: "github", url: "https://fixture.supabase.co/auth/v1/authorize?provider=github" },
      error: null,
    });
    await cloudSignInWithGitHub();
    expect(mocks.signInWithOAuth).toHaveBeenCalledWith({
      provider: "github",
      options: { redirectTo: "runory://auth/callback", skipBrowserRedirect: true },
    });
    expect(mocks.openUrl).toHaveBeenCalledExactlyOnceWith("https://fixture.supabase.co/auth/v1/authorize?provider=github");
  });

  it("rejects an authorization URL for a different provider", async () => {
    mocks.signInWithOAuth.mockResolvedValueOnce({
      data: { provider: "github", url: "https://fixture.supabase.co/auth/v1/authorize?provider=github" },
      error: null,
    });
    await expect(cloudSignInWithGoogle()).rejects.toThrow("OAUTH_URL_INVALID");
    expect(mocks.openUrl).not.toHaveBeenCalled();
  });

  it("exchanges a validated one-attempt callback into the main in-memory session", async () => {
    await cloudSignInWithGoogle();
    await expect(completeCloudOAuthRedirect("runory://auth/callback?code=one-use-code")).resolves.toEqual(session);
    expect(mocks.exchangeCodeForSession).toHaveBeenCalledExactlyOnceWith("one-use-code");
    expect(mocks.setSession).toHaveBeenCalledExactlyOnceWith({ access_token: "access-token", refresh_token: "refresh-token" });
    await expect(completeCloudOAuthRedirect("runory://auth/callback?code=replay")).rejects.toThrow("OAUTH_ATTEMPT_MISSING");
  });

  it("rejects untrusted and malformed callback URLs", async () => {
    expect(isCloudOAuthRedirect("runory://auth/callback?code=value")).toBe(true);
    expect(isCloudOAuthRedirect("runory://attacker/callback?code=value")).toBe(false);
    await expect(completeCloudOAuthRedirect("https://attacker.example/callback?code=value")).rejects.toThrow("OAUTH_REDIRECT_INVALID");
  });

  it("applies email-confirm handoff tokens from the deep-link fragment", async () => {
    const deepLink = buildCloudEmailConfirmDeepLink("access-token", "refresh-token");
    expect(deepLink.startsWith("runory://auth/callback#")).toBe(true);
    expect(deepLink).not.toContain("?access_token=");
    await expect(completeCloudAuthDeepLink(deepLink)).resolves.toEqual(session);
    expect(mocks.setSession).toHaveBeenCalledExactlyOnceWith({
      access_token: "access-token",
      refresh_token: "refresh-token",
    });
    expect(mocks.exchangeCodeForSession).not.toHaveBeenCalled();
  });

  it("rejects email-confirm tokens placed in the query string", async () => {
    await expect(
      completeCloudAuthDeepLink("runory://auth/callback?access_token=access&refresh_token=refresh"),
    ).rejects.toThrow("AUTH_DEEPLINK_TOKEN_IN_QUERY");
    expect(mocks.setSession).not.toHaveBeenCalled();
  });

  it("routes a pending OAuth code through completeCloudAuthDeepLink", async () => {
    await cloudSignInWithGoogle();
    await expect(completeCloudAuthDeepLink("runory://auth/callback?code=one-use-code")).resolves.toEqual(session);
    expect(mocks.exchangeCodeForSession).toHaveBeenCalledExactlyOnceWith("one-use-code");
  });
});
