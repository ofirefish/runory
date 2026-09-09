import { SearchAddon } from "@xterm/addon-search";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import { forwardRef, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState } from "react";
import { resizeSsh, writeSsh } from "../../lib/tauri/ssh";
import { TerminalInputBuffer } from "../../lib/terminal/input-buffer";
import { TerminalToolbar } from "./TerminalToolbar";
import { MobileExtraKeys } from "./MobileExtraKeys";

export type TerminalHandle = {
  write: (bytes: number[]) => void;
  dimensions: () => { cols: number; rows: number };
};

const darkSearchOptions = {
  incremental: true,
  decorations: {
    matchBackground: "#334155",
    matchOverviewRuler: "#64748b",
    activeMatchBackground: "#2563eb",
    activeMatchColorOverviewRuler: "#3b82f6",
  },
};

const lightSearchOptions = {
  incremental: true,
  decorations: {
    matchBackground: "#fde68a",
    matchOverviewRuler: "#d97706",
    activeMatchBackground: "#4f46e5",
    activeMatchColorOverviewRuler: "#6366f1",
  },
};

const darkTerminalTheme = {
  background: "#0f1a29",
  foreground: "#d7e0ec",
  cursor: "#94a3b8",
  cursorAccent: "#0f1a29",
  selectionBackground: "#33415599",
  black: "#0f172a",
  red: "#ef4444",
  green: "#10b981",
  yellow: "#f59e0b",
  blue: "#3b82f6",
  magenta: "#a78bfa",
  cyan: "#22d3ee",
  white: "#d7e0ec",
  brightBlack: "#64748b",
  brightRed: "#f87171",
  brightGreen: "#34d399",
  brightYellow: "#fbbf24",
  brightBlue: "#60a5fa",
  brightMagenta: "#c4b5fd",
  brightCyan: "#67e8f9",
  brightWhite: "#f8fafc",
};

const lightTerminalTheme = {
  background: "#f8fafc",
  foreground: "#1e293b",
  cursor: "#475569",
  cursorAccent: "#f8fafc",
  selectionBackground: "#c7d2fe99",
  black: "#0f172a",
  red: "#dc2626",
  green: "#15803d",
  yellow: "#a16207",
  blue: "#2563eb",
  magenta: "#7c3aed",
  cyan: "#0e7490",
  white: "#cbd5e1",
  brightBlack: "#64748b",
  brightRed: "#ef4444",
  brightGreen: "#16a34a",
  brightYellow: "#ca8a04",
  brightBlue: "#3b82f6",
  brightMagenta: "#8b5cf6",
  brightCyan: "#0891b2",
  brightWhite: "#94a3b8",
};

const currentTerminalTheme = () => document.documentElement.classList.contains("dark") ? darkTerminalTheme : lightTerminalTheme;
const currentSearchOptions = () => document.documentElement.classList.contains("dark") ? darkSearchOptions : lightSearchOptions;

export const TerminalView = forwardRef<TerminalHandle, { sessionId: string | null; active?: boolean; toolbarHost?: HTMLDivElement | null; onTransportError?: () => void }>(function TerminalView({ sessionId, active = true, toolbarHost, onTransportError }, ref) {
  const container = useRef<HTMLDivElement>(null);
  const terminal = useRef<Terminal | null>(null);
  const fit = useRef<FitAddon | null>(null);
  const search = useRef<SearchAddon | null>(null);
  const input = useRef<TerminalInputBuffer | null>(null);
  const session = useRef(sessionId);
  const isActive = useRef(active);
  const transportError = useRef(onTransportError);
  const [searchOpen, setSearchOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [searchResult, setSearchResult] = useState({ resultIndex: -1, resultCount: 0 });
  const [clipboardError, setClipboardError] = useState(false);

  useEffect(() => { transportError.current = onTransportError; }, [onTransportError]);
  useEffect(() => { isActive.current = active; }, [active]);
  useEffect(() => {
    input.current?.clear();
    session.current = sessionId;
  }, [sessionId]);
  useLayoutEffect(() => {
    if (!active) return;
    const term = terminal.current;
    const fitAddon = fit.current;
    if (!term || !fitAddon) return;
    fitAddon.fit();
    term.refresh(0, term.rows - 1);
    term.focus();
    const frame = requestAnimationFrame(() => {
      fitAddon.fit();
      term.refresh(0, term.rows - 1);
      if (session.current) {
        void resizeSsh(session.current, term.cols, term.rows).catch(() => transportError.current?.());
      }
      term.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [active]);
  useImperativeHandle(ref, () => ({
    write: (bytes) => terminal.current?.write(new Uint8Array(bytes)),
    dimensions: () => ({ cols: terminal.current?.cols ?? 120, rows: terminal.current?.rows ?? 34 }),
  }), []);

  const copySelection = async () => {
    const selection = terminal.current?.getSelection();
    if (!selection || !navigator.clipboard) return;
    try {
      await navigator.clipboard.writeText(selection);
      setClipboardError(false);
    } catch {
      setClipboardError(true);
    }
  };
  const paste = async () => {
    if (!navigator.clipboard) { setClipboardError(true); return; }
    try {
      terminal.current?.paste(await navigator.clipboard.readText());
      setClipboardError(false);
    } catch {
      setClipboardError(true);
    }
  };
  const findNext = (term = query) => {
    if (term) search.current?.findNext(term, currentSearchOptions());
  };
  const findPrevious = () => {
    if (query) search.current?.findPrevious(query, currentSearchOptions());
  };
  const closeSearch = () => {
    setSearchOpen(false);
    search.current?.clearDecorations();
    terminal.current?.focus();
  };
  const sendExtraKey = (value: string) => {
    if (session.current) input.current?.push(session.current, value);
    terminal.current?.focus();
  };

  useEffect(() => {
    if (!container.current) return;
    const term = new Terminal({ cursorBlink: true, fontFamily: '"JetBrains Mono", Consolas, monospace', fontSize: 14, scrollback: 10_000, convertEol: false, theme: currentTerminalTheme() });
    const fitAddon = new FitAddon();
    const searchAddon = new SearchAddon();
    const inputBuffer = new TerminalInputBuffer(writeSsh, () => transportError.current?.());
    term.loadAddon(fitAddon);
    term.loadAddon(searchAddon);
    term.open(container.current);
    fitAddon.fit();
    terminal.current = term;
    fit.current = fitAddon;
    search.current = searchAddon;
    input.current = inputBuffer;
    const searchResults = searchAddon.onDidChangeResults(setSearchResult);
    const data = term.onData((value) => { if (session.current) inputBuffer.push(session.current, value); });
    term.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown" || !(event.ctrlKey || event.metaKey) || !event.shiftKey) return true;
      const key = event.key.toLowerCase();
      if (key === "f") { setSearchOpen(true); return false; }
      if (key === "c") { void copySelection(); return false; }
      if (key === "v") { void paste(); return false; }
      return true;
    });
    const observer = new ResizeObserver(() => {
      if (!isActive.current) return;
      fitAddon.fit();
      if (session.current) void resizeSsh(session.current, term.cols, term.rows).catch(() => transportError.current?.());
    });
    observer.observe(container.current);
    const themeObserver = new MutationObserver(() => {
      term.options.theme = currentTerminalTheme();
      term.refresh(0, term.rows - 1);
    });
    themeObserver.observe(document.documentElement, { attributeFilter: ["class"] });
    return () => {
      observer.disconnect();
      themeObserver.disconnect();
      searchResults.dispose();
      data.dispose();
      inputBuffer.dispose();
      term.dispose();
      terminal.current = null;
      fit.current = null;
      search.current = null;
      input.current = null;
    };
  }, []);

  return <div className="terminal-view relative flex h-full min-h-0 w-full flex-col">
    <TerminalToolbar toolbarHost={toolbarHost} searchOpen={searchOpen} query={query} result={searchResult} clipboardError={clipboardError} onOpenSearch={() => setSearchOpen(true)} onCloseSearch={closeSearch} onQueryChange={(value) => { setQuery(value); findNext(value); }} onFindNext={() => findNext()} onFindPrevious={findPrevious} onCopy={() => void copySelection()} onPaste={() => void paste()} />
    <div
      ref={container}
      className="min-h-0 w-full flex-1"
      onContextMenu={(event) => {
        event.preventDefault();
        void paste();
      }}
    />
    <MobileExtraKeys disabled={!sessionId} onInput={sendExtraKey} />
  </div>;
});
