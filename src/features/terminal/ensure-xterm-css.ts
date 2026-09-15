import xtermCss from "@xterm/xterm/css/xterm.css?inline";

const STYLE_ID = "runory-xterm-css";

/**
 * Inject official xterm.css from the JS bundle so Tauri production never depends
 * on async <link rel="stylesheet"> timing.
 *
 * WebView2 has been observed to ignore `color: transparent` on the a11y layer
 * under the custom protocol, leaving opaque sans-serif text over the canvas
 * (looks like "no ANSI colors"). We always hide that layer — Runory keeps
 * screenReaderMode off.
 */
export function ensureXtermCssInjected(): void {
  if (typeof document === "undefined") return;
  if (document.getElementById(STYLE_ID)) return;
  const style = document.createElement("style");
  style.id = STYLE_ID;
  style.setAttribute("data-runory", "xterm");
  style.textContent = `${xtermCss}
/* Runory: never show the a11y DOM layer (canvas is the only renderer). */
.xterm .xterm-accessibility,
.xterm .xterm-accessibility-tree,
.xterm .xterm-message {
  display: none !important;
}
/* Defeat Tailwind preflight canvas constraints that break FitAddon metrics. */
.xterm canvas {
  display: block !important;
  max-width: none !important;
  image-rendering: auto;
}
.xterm .xterm-screen canvas {
  position: absolute !important;
  left: 0 !important;
  top: 0 !important;
  z-index: 1 !important;
}
`;
  document.head.appendChild(style);
}

ensureXtermCssInjected();
