import { createCloudAvatarUrl } from "./supabase/cloud";
import {
  cachedAvatarDataUrl,
  getCachedCloudAvatar,
  removeCachedCloudAvatar,
  storeCachedCloudAvatar,
} from "./tauri/cloud-avatar-cache";

export async function loadCloudAvatar(
  userId: string,
  avatarPath: string,
  avatarVersion: number,
): Promise<string> {
  const cached = await getCachedCloudAvatar(userId, avatarVersion);
  if (cached) return cachedAvatarDataUrl(cached);
  const signedUrl = await createCloudAvatarUrl(avatarPath);
  return cachedAvatarDataUrl(await storeCachedCloudAvatar(userId, avatarVersion, signedUrl));
}

export async function loadLatestCachedCloudAvatar(userId: string): Promise<string | null> {
  const cached = await getCachedCloudAvatar(userId);
  return cached ? cachedAvatarDataUrl(cached) : null;
}

export async function clearCloudAvatarCache(userId: string): Promise<void> {
  await removeCachedCloudAvatar(userId);
}
