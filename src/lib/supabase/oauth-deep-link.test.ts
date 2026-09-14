import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  completeCloudAuthDeepLink: vi.fn(),
  getCurrent: vi.fn(),
  onOpenUrl: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-deep-link", () => ({
  getCurrent: mocks.getCurrent,
  onOpenUrl: mocks.onOpenUrl,
}));
vi.mock("./cloud", () => ({
  cloudOAuthErrorEvent: "runory:cloud-oauth-error",
  completeCloudAuthDeepLink: mocks.completeCloudAuthDeepLink,
  isCloudOAuthRedirect: (value: string) => value.startsWith("runory://auth/callback"),
}));

describe("initializeCloudOAuthDeepLinks", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getCurrent.mockResolvedValue(null);
    mocks.onOpenUrl.mockResolvedValue(undefined);
    mocks.completeCloudAuthDeepLink.mockResolvedValue({});
  });

  it("consumes email-confirm deep links through completeCloudAuthDeepLink", async () => {
    const deepLink = "runory://auth/callback#access_token=a&refresh_token=b&type=email_confirm";
    mocks.getCurrent.mockResolvedValueOnce([deepLink]);
    const { initializeCloudOAuthDeepLinks } = await import("./oauth-deep-link");
    await initializeCloudOAuthDeepLinks();
    expect(mocks.completeCloudAuthDeepLink).toHaveBeenCalledExactlyOnceWith(deepLink);
  });
});
