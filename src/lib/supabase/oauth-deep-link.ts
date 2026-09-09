import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { cloudOAuthErrorEvent, completeCloudOAuthRedirect, isCloudOAuthRedirect } from "./cloud";

const consumed = new Set<string>();

function reportFailure() {
  window.dispatchEvent(new Event(cloudOAuthErrorEvent));
}

async function consume(urls: string[]) {
  for (const value of urls) {
    if (!isCloudOAuthRedirect(value) || consumed.has(value)) continue;
    consumed.add(value);
    try {
      await completeCloudOAuthRedirect(value);
    } catch {
      reportFailure();
    }
  }
}

export async function initializeCloudOAuthDeepLinks(): Promise<void> {
  await onOpenUrl((urls) => { void consume(urls); });
  const current = await getCurrent();
  if (current) await consume(current);
}
