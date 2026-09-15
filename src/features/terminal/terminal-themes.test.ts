import { describe, expect, it } from "vitest";
import {
  TERMINAL_THEME_IDS,
  chromaticSlotsDistinct,
  resolveTerminalPalette,
} from "./terminal-themes";

describe("terminal theme presets", () => {
  it("keeps chromatic ANSI slots distinct for file-type coloring", () => {
    for (const id of TERMINAL_THEME_IDS) {
      expect(chromaticSlotsDistinct(resolveTerminalPalette(id, true)), `${id}/dark`).toBe(true);
      expect(chromaticSlotsDistinct(resolveTerminalPalette(id, false)), `${id}/light`).toBe(true);
      const dark = resolveTerminalPalette(id, true);
      const light = resolveTerminalPalette(id, false);
      expect(dark.background).toMatch(/^#/);
      expect(light.background).toMatch(/^#/);
      expect(dark.background).not.toBe(light.background);
    }
  });

  it("falls back to Runory for unknown ids", () => {
    const runory = resolveTerminalPalette("runory", true);
    expect(resolveTerminalPalette("unknown" as "runory", true)).toEqual(runory);
  });
});
