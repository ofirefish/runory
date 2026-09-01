const REMOTE_PATH_KEY_PREFIX = "runory.files.lastPath.";
const MAX_REMOTE_PATH_LENGTH = 4096;

type PathStorage = Pick<Storage, "getItem" | "setItem" | "removeItem">;

function browserStorage(): PathStorage | null {
  if (typeof window === "undefined") return null;
  try { return window.localStorage; } catch { return null; }
}

const storageKey = (profileId: string) => `${REMOTE_PATH_KEY_PREFIX}${encodeURIComponent(profileId)}`;

export function readLastRemotePath(profileId: string, storage: PathStorage | null = browserStorage()): string | null {
  if (!storage) return null;
  try {
    const path = storage.getItem(storageKey(profileId));
    return path?.startsWith("/") && path.length <= MAX_REMOTE_PATH_LENGTH ? path : null;
  } catch {
    return null;
  }
}

export function saveLastRemotePath(profileId: string, path: string, storage: PathStorage | null = browserStorage()): void {
  if (!storage || !path.startsWith("/") || path.length > MAX_REMOTE_PATH_LENGTH) return;
  try { storage.setItem(storageKey(profileId), path); } catch { /* UI history is best-effort. */ }
}

export function clearLastRemotePath(profileId: string, storage: PathStorage | null = browserStorage()): void {
  if (!storage) return;
  try { storage.removeItem(storageKey(profileId)); } catch { /* UI history is best-effort. */ }
}
