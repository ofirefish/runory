import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAppUpdateStore } from "./app-update-store";

const api = vi.hoisted(() => ({
  status: vi.fn(),
  check: vi.fn(),
  download: vi.fn(),
  install: vi.fn(),
}));

vi.mock("../lib/tauri/app-update", () => ({
  appUpdateStatus: api.status,
  checkForAppUpdate: api.check,
  downloadAppUpdate: api.download,
  installAppUpdate: api.install,
}));

const update = {
  currentVersion: "0.1.0",
  version: "0.2.0",
  notes: "Security fixes",
  date: "2026-09-07T00:00:00Z",
};

beforeEach(() => {
  vi.clearAllMocks();
  useAppUpdateStore.setState({
    phase: "idle",
    configured: false,
    currentVersion: null,
    update: null,
    downloadedBytes: 0,
    totalBytes: null,
    errorCode: null,
  });
});

describe("desktop app updates", () => {
  it("automatically checks and downloads a signed update", async () => {
    api.status.mockResolvedValue({ configured: true, currentVersion: "0.1.0", update: null, downloaded: false });
    api.check.mockResolvedValue({ configured: true, currentVersion: "0.1.0", update, downloaded: false });
    api.download.mockImplementation(async (onEvent: (event: object) => void) => {
      onEvent({ event: "progress", downloadedBytes: 50, totalBytes: 100 });
      return { configured: true, currentVersion: "0.1.0", update, downloaded: true };
    });

    await useAppUpdateStore.getState().initialize();

    expect(api.check).toHaveBeenCalledOnce();
    expect(api.download).toHaveBeenCalledOnce();
    expect(useAppUpdateStore.getState()).toMatchObject({
      phase: "ready",
      update,
      downloadedBytes: 50,
      totalBytes: 100,
    });
  });

  it("requires a second explicit action before disconnecting active sessions", async () => {
    useAppUpdateStore.setState({ phase: "ready", configured: true, currentVersion: "0.1.0", update });
    api.install.mockRejectedValueOnce({ code: "UPDATE_BUSY" }).mockResolvedValueOnce(undefined);

    await useAppUpdateStore.getState().install(false);
    expect(useAppUpdateStore.getState().phase).toBe("busy");

    await useAppUpdateStore.getState().install(true);
    expect(api.install).toHaveBeenNthCalledWith(1, false);
    expect(api.install).toHaveBeenNthCalledWith(2, true);
  });
});
