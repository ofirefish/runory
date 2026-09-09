import { beforeEach, expect, it, vi } from "vitest";
import { createCloudAvatarUrl } from "./supabase/cloud";
import { getCachedCloudAvatar, storeCachedCloudAvatar } from "./tauri/cloud-avatar-cache";
import { loadCloudAvatar, loadLatestCachedCloudAvatar } from "./cloud-avatar";

vi.mock("./supabase/cloud", () => ({ createCloudAvatarUrl: vi.fn() }));
vi.mock("./tauri/cloud-avatar-cache", () => ({
  cachedAvatarDataUrl: ({ mimeType, dataBase64 }: { mimeType: string; dataBase64: string }) =>
    `data:${mimeType};base64,${dataBase64}`,
  getCachedCloudAvatar: vi.fn(),
  removeCachedCloudAvatar: vi.fn(),
  storeCachedCloudAvatar: vi.fn(),
}));

beforeEach(() => vi.clearAllMocks());

it("uses the local avatar cache without requesting a signed URL", async () => {
  vi.mocked(getCachedCloudAvatar).mockResolvedValue({ mimeType: "image/webp", dataBase64: "cached" });
  await expect(loadCloudAvatar("user-1", "user-1/avatar.webp", 3)).resolves.toBe(
    "data:image/webp;base64,cached",
  );
  expect(createCloudAvatarUrl).not.toHaveBeenCalled();
  expect(storeCachedCloudAvatar).not.toHaveBeenCalled();
});

it("downloads and stores a cache miss", async () => {
  vi.mocked(getCachedCloudAvatar).mockResolvedValue(null);
  vi.mocked(createCloudAvatarUrl).mockResolvedValue("https://project.supabase.co/signed-avatar");
  vi.mocked(storeCachedCloudAvatar).mockResolvedValue({ mimeType: "image/webp", dataBase64: "fresh" });
  await expect(loadCloudAvatar("user-1", "user-1/avatar.webp", 4)).resolves.toBe(
    "data:image/webp;base64,fresh",
  );
  expect(storeCachedCloudAvatar).toHaveBeenCalledWith(
    "user-1",
    4,
    "https://project.supabase.co/signed-avatar",
  );
});

it("loads the latest local avatar without a network request", async () => {
  vi.mocked(getCachedCloudAvatar).mockResolvedValue({ mimeType: "image/png", dataBase64: "offline" });
  await expect(loadLatestCachedCloudAvatar("user-1")).resolves.toBe("data:image/png;base64,offline");
  expect(getCachedCloudAvatar).toHaveBeenCalledWith("user-1");
  expect(createCloudAvatarUrl).not.toHaveBeenCalled();
});
