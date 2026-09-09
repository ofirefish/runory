// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { cloudSyncKeyStatus, exportCloudSync } from "../../lib/tauri/cloud";
import { cloudSession, listOrganizations, loadEncryptedInventory, writeEncryptedInventory } from "../../lib/supabase/cloud";
import { ServerSyncStatus } from "./ServerSyncStatus";

vi.mock("../../lib/supabase/client", () => ({ cloudConfigured: true }));
vi.mock("../../lib/supabase/cloud", () => ({
  cloudSession: vi.fn(),
  listOrganizations: vi.fn(),
  loadEncryptedInventory: vi.fn(),
  writeEncryptedInventory: vi.fn(),
  onCloudAuthStateChange: vi.fn(() => ({ unsubscribe: vi.fn() })),
}));
vi.mock("../../lib/tauri/cloud", () => ({
  cloudSyncKeyStatus: vi.fn(),
  discardCloudSync: vi.fn(),
  exportCloudSync: vi.fn(),
  previewCloudSync: vi.fn(),
}));

let container: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  vi.clearAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  await i18n.changeLanguage("en-US");
  vi.mocked(cloudSession).mockResolvedValue({ user: { id: "user-1" } } as Awaited<ReturnType<typeof cloudSession>>);
  vi.mocked(listOrganizations).mockResolvedValue([{ id: "organization-1", name: "Personal", kind: "personal", owner_id: "user-1", created_at: "", updated_at: "" }]);
  vi.mocked(loadEncryptedInventory).mockResolvedValue(null);
  vi.mocked(cloudSyncKeyStatus).mockResolvedValue({ configured: true, persistedOnDevice: true, secureStorageAvailable: true });
  vi.mocked(exportCloudSync).mockResolvedValue({ version: 4, salt: [], nonce: [], ciphertext: [] });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  vi.unstubAllGlobals();
});

describe("server sync status", () => {
  it("syncs directly when the device already has a configured sync key", async () => {
    const openSync = vi.fn();
    await act(async () => {
      root.render(<ServerSyncStatus onOpenSync={openSync} onOpenAuth={vi.fn()} />);
    });
    const button = container.querySelector<HTMLButtonElement>(`button[title="${i18n.t("serverManagement.syncNow")}"]`);
    expect(button).not.toBeNull();

    await act(async () => { button?.click(); });

    expect(exportCloudSync).toHaveBeenCalledExactlyOnceWith("organization-1");
    expect(writeEncryptedInventory).toHaveBeenCalledExactlyOnceWith("organization-1", { version: 4, salt: [], nonce: [], ciphertext: [] }, 0);
    expect(openSync).not.toHaveBeenCalled();
    expect(container.querySelector(".server-sync-status")?.textContent).toContain(i18n.t("serverManagement.syncStatus.synced"));
  });
});
