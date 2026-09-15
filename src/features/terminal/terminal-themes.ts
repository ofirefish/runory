import type { ITheme } from "@xterm/xterm";
import type { TerminalThemeId } from "../../lib/tauri/settings";

export type TerminalPalette = Required<
  Pick<
    ITheme,
    | "background"
    | "foreground"
    | "cursor"
    | "cursorAccent"
    | "selectionBackground"
    | "black"
    | "red"
    | "green"
    | "yellow"
    | "blue"
    | "magenta"
    | "cyan"
    | "white"
    | "brightBlack"
    | "brightRed"
    | "brightGreen"
    | "brightYellow"
    | "brightBlue"
    | "brightMagenta"
    | "brightCyan"
    | "brightWhite"
  >
>;

export const TERMINAL_THEME_IDS = [
  "runory",
  "oneDark",
  "tokyoNight",
  "catppuccin",
  "solarized",
] as const satisfies readonly TerminalThemeId[];

type ThemePair = { dark: TerminalPalette; light: TerminalPalette };

/**
 * ANSI roles used by common dircolors / prompts:
 * red → archives / errors · green → executables · yellow → devices / warnings
 * blue → directories · magenta → images / sockets · cyan → symlinks / special
 * Bright variants must differ from normal so bold file types stay distinct.
 */

/** Runory slate — wide hue gaps on product navy. */
const runory: ThemePair = {
  dark: {
    background: "#0b1524",
    foreground: "#e8eef7",
    cursor: "#7dd3fc",
    cursorAccent: "#0b1524",
    selectionBackground: "#2563eb55",
    black: "#1e293b",
    red: "#f43f5e",
    green: "#22c55e",
    yellow: "#eab308",
    blue: "#3b82f6",
    magenta: "#d946ef",
    cyan: "#06b6d4",
    white: "#e2e8f0",
    brightBlack: "#94a3b8",
    brightRed: "#fb7185",
    brightGreen: "#4ade80",
    brightYellow: "#facc15",
    brightBlue: "#60a5fa",
    brightMagenta: "#e879f9",
    brightCyan: "#22d3ee",
    brightWhite: "#ffffff",
  },
  light: {
    background: "#f8fafc",
    foreground: "#0f172a",
    cursor: "#2563eb",
    cursorAccent: "#f8fafc",
    selectionBackground: "#93c5fd66",
    black: "#0f172a",
    red: "#be123c",
    green: "#047857",
    yellow: "#b45309",
    blue: "#1d4ed8",
    magenta: "#a21caf",
    cyan: "#0e7490",
    white: "#64748b",
    brightBlack: "#475569",
    brightRed: "#e11d48",
    brightGreen: "#059669",
    brightYellow: "#d97706",
    brightBlue: "#2563eb",
    brightMagenta: "#c026d3",
    brightCyan: "#0891b2",
    brightWhite: "#0f172a",
  },
};

/** One Dark — keep Atom hues, split bright for bold dircolors. */
const oneDark: ThemePair = {
  dark: {
    background: "#282c34",
    foreground: "#abb2bf",
    cursor: "#528bff",
    cursorAccent: "#282c34",
    selectionBackground: "#3e4451aa",
    black: "#3f4451",
    red: "#e06c75",
    green: "#98c379",
    yellow: "#e5c07b",
    blue: "#61afef",
    magenta: "#c678dd",
    cyan: "#56b6c2",
    white: "#abb2bf",
    brightBlack: "#7f848e",
    brightRed: "#ff7b86",
    brightGreen: "#b5e890",
    brightYellow: "#f0d48a",
    brightBlue: "#7cc2ff",
    brightMagenta: "#d898f0",
    brightCyan: "#6ad4df",
    brightWhite: "#ffffff",
  },
  light: {
    background: "#fafafa",
    foreground: "#383a42",
    cursor: "#526fff",
    cursorAccent: "#fafafa",
    selectionBackground: "#e5e5e6aa",
    black: "#383a42",
    red: "#e45649",
    green: "#50a14f",
    yellow: "#c18401",
    blue: "#4078f2",
    magenta: "#a626a4",
    cyan: "#0184bc",
    white: "#a0a1a7",
    brightBlack: "#696c77",
    brightRed: "#ff6b5b",
    brightGreen: "#69b868",
    brightYellow: "#e0a000",
    brightBlue: "#5a8fff",
    brightMagenta: "#c040be",
    brightCyan: "#06a0d8",
    brightWhite: "#090a0b",
  },
};

/** Tokyo Night — cooler blues/cyan, warmer rose/gold separation. */
const tokyoNight: ThemePair = {
  dark: {
    background: "#1a1b26",
    foreground: "#c0caf5",
    cursor: "#c0caf5",
    cursorAccent: "#1a1b26",
    selectionBackground: "#33467caa",
    black: "#15161e",
    red: "#f7768e",
    green: "#9ece6a",
    yellow: "#e0af68",
    blue: "#7aa2f7",
    magenta: "#bb9af7",
    cyan: "#7dcfff",
    white: "#a9b1d6",
    brightBlack: "#565f89",
    brightRed: "#ff9db0",
    brightGreen: "#b9f27c",
    brightYellow: "#ffd9a0",
    brightBlue: "#9db9ff",
    brightMagenta: "#d0b3ff",
    brightCyan: "#a4e8ff",
    brightWhite: "#ffffff",
  },
  light: {
    background: "#e1e2e7",
    foreground: "#3760bf",
    cursor: "#3760bf",
    cursorAccent: "#e1e2e7",
    selectionBackground: "#b7c1e3aa",
    black: "#343b58",
    red: "#f52a65",
    green: "#587539",
    yellow: "#8c6c3e",
    blue: "#2e7de9",
    magenta: "#9854f1",
    cyan: "#007197",
    white: "#6172b0",
    brightBlack: "#9699a3",
    brightRed: "#ff4d7a",
    brightGreen: "#6a8f45",
    brightYellow: "#a67c3a",
    brightBlue: "#4a93f5",
    brightMagenta: "#b06dff",
    brightCyan: "#0a8fb5",
    brightWhite: "#1a1b26",
  },
};

/** Catppuccin — preserve Mocha/Latte accents with brighter bold steps. */
const catppuccin: ThemePair = {
  dark: {
    background: "#1e1e2e",
    foreground: "#cdd6f4",
    cursor: "#f5e0dc",
    cursorAccent: "#1e1e2e",
    selectionBackground: "#585b7088",
    black: "#45475a",
    red: "#f38ba8",
    green: "#a6e3a1",
    yellow: "#f9e2af",
    blue: "#89b4fa",
    magenta: "#cba6f7",
    cyan: "#94e2d5",
    white: "#bac2de",
    brightBlack: "#6c7086",
    brightRed: "#ffa0b8",
    brightGreen: "#b8f0b3",
    brightYellow: "#ffe6c0",
    brightBlue: "#a6c8ff",
    brightMagenta: "#ddb8ff",
    brightCyan: "#aef0e4",
    brightWhite: "#ffffff",
  },
  light: {
    background: "#eff1f5",
    foreground: "#4c4f69",
    cursor: "#dc8a78",
    cursorAccent: "#eff1f5",
    selectionBackground: "#acb0be88",
    black: "#5c5f77",
    red: "#d20f39",
    green: "#40a02b",
    yellow: "#df8e1d",
    blue: "#1e66f5",
    magenta: "#8839ef",
    cyan: "#179299",
    white: "#acb0be",
    brightBlack: "#6c6f85",
    brightRed: "#e6455f",
    brightGreen: "#4fbf37",
    brightYellow: "#f0a030",
    brightBlue: "#3b7dff",
    brightMagenta: "#9b57ff",
    brightCyan: "#1fadb5",
    brightWhite: "#4c4f69",
  },
};

/**
 * Solarized accents kept; bright slots use lighter/warmer companions
 * instead of classic base-tone remaps so ls/file types stay readable.
 */
const solarized: ThemePair = {
  dark: {
    background: "#002b36",
    foreground: "#839496",
    cursor: "#93a1a1",
    cursorAccent: "#002b36",
    selectionBackground: "#073642aa",
    black: "#073642",
    red: "#dc322f",
    green: "#859900",
    yellow: "#b58900",
    blue: "#268bd2",
    magenta: "#d33682",
    cyan: "#2aa198",
    white: "#eee8d5",
    brightBlack: "#586e75",
    brightRed: "#ff6b4a",
    brightGreen: "#a4b820",
    brightYellow: "#d9a400",
    brightBlue: "#4ba3e3",
    brightMagenta: "#e85aa0",
    brightCyan: "#3ecfbf",
    brightWhite: "#fdf6e3",
  },
  light: {
    background: "#fdf6e3",
    foreground: "#657b83",
    cursor: "#586e75",
    cursorAccent: "#fdf6e3",
    selectionBackground: "#eee8d5aa",
    black: "#073642",
    red: "#dc322f",
    green: "#859900",
    yellow: "#b58900",
    blue: "#268bd2",
    magenta: "#d33682",
    cyan: "#2aa198",
    white: "#93a1a1",
    brightBlack: "#586e75",
    brightRed: "#cb4b16",
    brightGreen: "#6d7c00",
    brightYellow: "#9a7300",
    brightBlue: "#1a6fad",
    brightMagenta: "#b0286c",
    brightCyan: "#1f857c",
    brightWhite: "#002b36",
  },
};

const PRESETS: Record<TerminalThemeId, ThemePair> = {
  runory,
  oneDark,
  tokyoNight,
  catppuccin,
  solarized,
};

const CHROMATIC = [
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
] as const;

export function resolveTerminalPalette(id: TerminalThemeId, dark: boolean): TerminalPalette {
  const pair = PRESETS[id] ?? PRESETS.runory;
  return dark ? pair.dark : pair.light;
}

/** Keep workspace chrome around the xterm padding aligned with the active palette. */
export function applyTerminalSurface(background: string) {
  document.documentElement.style.setProperty("--terminal-surface", background);
}

/** Used by tests to ensure bold/file-type colors stay distinct. */
export function chromaticSlotsDistinct(palette: TerminalPalette): boolean {
  const values = CHROMATIC.map((key) => palette[key]);
  const bright = CHROMATIC.map((key) => palette[`bright${key[0].toUpperCase()}${key.slice(1)}` as keyof TerminalPalette]);
  const unique = new Set(values);
  if (unique.size !== values.length) return false;
  return CHROMATIC.every((_, index) => values[index] !== bright[index]);
}
