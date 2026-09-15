import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // Relative URLs so Tauri's custom protocol can resolve CSS/font/JS chunks after packaging.
  base: "./",
  server: { port: 1420, strictPort: true },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    // Prefer Chromium on Windows even when `pnpm build` runs outside `tauri build`
    // (missing TAURI_ENV_PLATFORM used to fall through to safari13 and break xterm).
    target:
      process.env.TAURI_ENV_PLATFORM === "windows" || process.platform === "win32"
        ? "chrome105"
        : "safari13",
    emptyOutDir: true,
    cssCodeSplit: false,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return;
          if (/\.(css|scss|sass|less)([?#]|$)/i.test(id)) return;
          // Keep @xterm in the same graph as TerminalView — splitting it caused
          // production-only rendering differences vs Vite dev (no colors / wrong font).
          if (id.includes("@xterm")) return;
          if (id.includes("@supabase")) return "vendor-supabase";
          if (id.includes("i18next") || id.includes("react-i18next")) return "vendor-i18n";
          if (id.includes("lucide-react")) return "vendor-lucide";
          if (id.includes("react-dom") || id.includes("scheduler") || /[/\\](react)[/\\]/.test(id)) {
            return "vendor-react";
          }
        },
      },
    },
  },
});
