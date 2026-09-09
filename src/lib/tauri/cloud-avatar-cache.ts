import { invoke } from "@tauri-apps/api/core";

type CachedCloudAvatar = {
  mimeType: string;
  dataBase64: string;
};

export const getCachedCloudAvatar = (userId: string, avatarVersion?: number) =>
  invoke<CachedCloudAvatar | null>("cloud_avatar_cache_get", { request: { userId, avatarVersion } });

export const storeCachedCloudAvatar = (userId: string, avatarVersion: number, signedUrl: string) =>
  invoke<CachedCloudAvatar>("cloud_avatar_cache_store", { request: { userId, avatarVersion, signedUrl } });

export const removeCachedCloudAvatar = (userId: string) =>
  invoke<void>("cloud_avatar_cache_remove", { userId });

export function cachedAvatarDataUrl(avatar: CachedCloudAvatar): string {
  return `data:${avatar.mimeType};base64,${avatar.dataBase64}`;
}
