import { isTauri } from "@tauri-apps/api/core";
/** Must run before React mounts so xterm a11y CSS exists before any Terminal.open. */
import "./features/terminal/ensure-xterm-css";

const SPLASH_MIN_MS = 480;
const splashShownAt = performance.now();

/**
 * Show the main window as soon as the HTML splash is available.
 * The heavy app module loads afterward so users see branded chrome instead of a white frame.
 */
async function revealSplashWindow() {
  if (!isTauri()) return;
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().show();
  } catch {
    // Browser / already-visible / capabilities not ready yet.
  }
}

/** Fade out the HTML splash after first paint; keep a short floor so it never flashes. */
function dismissBootSplash() {
  const splash = document.getElementById("boot-splash");
  if (!splash || splash.hasAttribute("hidden")) return;

  const remaining = Math.max(0, SPLASH_MIN_MS - (performance.now() - splashShownAt));
  window.setTimeout(() => {
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduceMotion) {
      splash.setAttribute("hidden", "");
      return;
    }
    splash.classList.add("is-leaving");
    window.setTimeout(() => splash.setAttribute("hidden", ""), 340);
  }, remaining);
}

void revealSplashWindow();
import("./main").then(() => {
  // main.tsx schedules its own window reveal; splash dismiss waits for React root paint.
  requestAnimationFrame(() => {
    requestAnimationFrame(() => dismissBootSplash());
  });
});
