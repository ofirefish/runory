// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  setSession: vi.fn(),
  onAuthStateChange: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, isTauri: () => true }));
vi.mock("./client", () => ({
  supabase: { auth: { setSession: mocks.setSession, onAuthStateChange: mocks.onAuthStateChange } },
}));

describe("secure cloud session persistence", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
    mocks.onAuthStateChange.mockReturnValue({ data: { subscription: { unsubscribe: vi.fn() } } });
  });

  it("restores a protected session through Supabase setSession", async () => {
    mocks.invoke.mockResolvedValueOnce({ accessToken: "access", refreshToken: "refresh" });
    mocks.setSession.mockResolvedValue({ data: { session: { access_token: "access" } }, error: null });
    const { initializeCloudSessionPersistence } = await import("./session-persistence");
    await initializeCloudSessionPersistence();
    expect(mocks.invoke).toHaveBeenCalledWith("cloud_auth_session_load");
    expect(mocks.setSession).toHaveBeenCalledWith({ access_token: "access", refresh_token: "refresh" });
  });

  it("writes only to the business-specific native session command", async () => {
    mocks.invoke.mockResolvedValue(undefined);
    const { persistCloudSession } = await import("./session-persistence");
    await expect(persistCloudSession({ access_token: "access", refresh_token: "refresh" } as never)).resolves.toBe(true);
    expect(mocks.invoke).toHaveBeenCalledWith("cloud_auth_session_save", {
      session: { accessToken: "access", refreshToken: "refresh" },
    });
    expect(window.localStorage).toHaveLength(0);
  });
});
