import type { Terminal } from "@xterm/xterm";

/** Drop any a11y DOM xterm may have created; Runory renders via canvas/WebGL only. */
export function suppressTerminalAccessibility(host: HTMLElement): void {
  host.querySelectorAll(".xterm-accessibility, .xterm-message").forEach((node) => {
    node.remove();
  });
}

/**
 * Prefer WebGL (explicit RGBA glyphs). Falls back to the built-in canvas renderer.
 * Must be called after `term.open()`.
 */
export function attachTerminalRenderer(term: Terminal): { dispose: () => void } {
  let disposed = false;
  let addon: { dispose: () => void } | null = null;

  void import("@xterm/addon-webgl")
    .then(({ WebglAddon }) => {
      if (disposed) return;
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => {
        try {
          webgl.dispose();
        } catch {
          // Renderer already torn down with the terminal.
        }
      });
      term.loadAddon(webgl);
      addon = webgl;
    })
    .catch(() => {
      // WebGL unavailable — keep the built-in canvas renderer.
    });

  return {
    dispose: () => {
      disposed = true;
      try {
        addon?.dispose();
      } catch {
        // ignore
      }
    },
  };
}

/** Confirm at least one screen canvas has a measurable box after open/fit. */
export function terminalCanvasIsHealthy(host: HTMLElement): boolean {
  const canvases = host.querySelectorAll(".xterm-screen canvas");
  for (const node of canvases) {
    const canvas = node as HTMLCanvasElement;
    if (canvas.clientWidth > 0 && canvas.clientHeight > 0) return true;
  }
  return false;
}
