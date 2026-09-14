import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { authDeepLinkDebugInfo, authErrorDebugInfo, logAuthDebug } from "./auth-debug";
import { cloudOAuthErrorEvent, completeCloudAuthDeepLink, isCloudOAuthRedirect } from "./cloud";

const consumed = new Set<string>();

function reportFailure() {
  window.dispatchEvent(new Event(cloudOAuthErrorEvent));
}

async function consume(urls: string[]) {
  for (const value of urls) {
    if (!isCloudOAuthRedirect(value) || consumed.has(value)) continue;
    consumed.add(value);
    logAuthDebug("deepLink:consume", authDeepLinkDebugInfo(value));
    try {
      await completeCloudAuthDeepLink(value);
    } catch (error) {
      logAuthDebug("deepLink:consume-error", authErrorDebugInfo(error));
      reportFailure();
    }
  }
}

export async function initializeCloudOAuthDeepLinks(): Promise<void> {
  await onOpenUrl((urls) => { void consume(urls); });
  const current = await getCurrent();
  if (current) await consume(current);
}
