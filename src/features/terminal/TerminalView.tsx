import { SearchAddon } from "@xterm/addon-search";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import { forwardRef, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState } from "react";
import { resizeSsh, writeSsh } from "../../lib/tauri/ssh";
import { TerminalInputBuffer } from "../../lib/terminal/input-buffer";
import { useSettingsStore } from "../../stores/settings-store";
import { ensureXtermCssInjected } from "./ensure-xterm-css";
import { TerminalOutputBuffer, toUint8Array } from "./output-buffer";
import {
  attachTerminalRenderer,
  suppressTerminalAccessibility,
  terminalCanvasIsHealthy,
} from "./terminal-renderer";
import { applyTerminalSurface, resolveTerminalPalette } from "./terminal-themes";
import { TerminalToolbar } from "./TerminalToolbar";
import { MobileExtraKeys } from "./MobileExtraKeys";

export type TerminalHandle = {
  write: (bytes: number[] | Uint8Array) => void;
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

const isDocumentDark = () => document.documentElement.classList.contains("dark");
const currentSearchOptions = () => (isDocumentDark() ? darkSearchOptions : lightSearchOptions);

function waitForVisibleBox(element: HTMLElement, isCancelled: () => boolean): Promise<boolean> {
  if (element.clientWidth > 0 && element.clientHeight > 0) return Promise.resolve(true);
  return new Promise((resolve) => {
    const finish = (ok: boolean) => {
      observer.disconnect();
      window.clearInterval(timer);
      resolve(ok);
    };
    const observer = new ResizeObserver(() => {
      if (element.clientWidth > 0 && element.clientHeight > 0) finish(true);
    });
    observer.observe(element);
    const timer = window.setInterval(() => {
      if (isCancelled()) {
        finish(false);
        return;
      }
      if (element.clientWidth > 0 && element.clientHeight > 0) finish(true);
    }, 50);
  });
}

export const TerminalView = forwardRef<TerminalHandle, { sessionId: string | null; active?: boolean; toolbarHost?: HTMLDivElement | null; onTransportError?: () => void }>(function TerminalView({ sessionId, active = true, toolbarHost, onTransportError }, ref) {
  const container = useRef<HTMLDivElement>(null);
  const terminal = useRef<Terminal | null>(null);
  const fit = useRef<FitAddon | null>(null);
  const search = useRef<SearchAddon | null>(null);
  const input = useRef<TerminalInputBuffer | null>(null);
  const output = useRef(new TerminalOutputBuffer());
  const session = useRef(sessionId);
  const isActive = useRef(active);
  const transportError = useRef(onTransportError);
  const terminalThemeId = useSettingsStore((state) => state.terminalTheme);
  const terminalThemeIdRef = useRef(terminalThemeId);
  const [searchOpen, setSearchOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [searchResult, setSearchResult] = useState({ resultIndex: -1, resultCount: 0 });
  const [clipboardError, setClipboardError] = useState(false);

  const applyPalette = (term: Terminal, id = terminalThemeIdRef.current) => {
    const palette = resolveTerminalPalette(id, isDocumentDark());
    term.options.theme = { ...palette };
    applyTerminalSurface(palette.background);
    term.refresh(0, term.rows - 1);
  };

  useEffect(() => { transportError.current = onTransportError; }, [onTransportError]);
  useEffect(() => { isActive.current = active; }, [active]);
  useEffect(() => {
    input.current?.clear();
    session.current = sessionId;
  }, [sessionId]);
  useEffect(() => {
    terminalThemeIdRef.current = terminalThemeId;
    const term = terminal.current;
    if (term) applyPalette(term, terminalThemeId);
  }, [terminalThemeId]);
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
    write: (bytes) => output.current.write(toUint8Array(bytes)),
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
    let cancelled = false;
    const host = container.current;
    const outputBuffer = output.current;
    ensureXtermCssInjected();

    const start = async () => {
      const visible = await waitForVisibleBox(host, () => cancelled);
      if (cancelled || !host.isConnected || !visible) return;

      try {
        await document.fonts.load('14px "JetBrains Mono"');
        await document.fonts.ready;
      } catch {
        // System monospace metrics are acceptable if the webfont is unavailable.
      }
      if (cancelled || !host.isConnected) return;

      const initial = resolveTerminalPalette(terminalThemeIdRef.current, isDocumentDark());
      applyTerminalSurface(initial.background);
      const term = new Terminal({
        cursorBlink: true,
        fontFamily: '"JetBrains Mono", ui-monospace, Consolas, "Courier New", monospace',
        fontSize: 14,
        scrollback: 10_000,
        convertEol: false,
        screenReaderMode: false,
        allowProposedApi: false,
        theme: { ...initial },
      });
      const fitAddon = new FitAddon();
      const searchAddon = new SearchAddon();
      const inputBuffer = new TerminalInputBuffer(writeSsh, () => transportError.current?.());
      term.loadAddon(fitAddon);
      term.loadAddon(searchAddon);
      term.open(host);
      suppressTerminalAccessibility(host);
      const renderer = attachTerminalRenderer(term);
      fitAddon.fit();
      applyPalette(term);
      if (!terminalCanvasIsHealthy(host)) {
        fitAddon.fit();
        term.refresh(0, term.rows - 1);
      }
      suppressTerminalAccessibility(host);
      terminal.current = term;
      fit.current = fitAddon;
      search.current = searchAddon;
      input.current = inputBuffer;
      outputBuffer.attach((chunk) => term.write(chunk));

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
      observer.observe(host);
      const themeObserver = new MutationObserver(() => applyPalette(term));
      themeObserver.observe(document.documentElement, { attributeFilter: ["class"] });

      cleanup = () => {
        observer.disconnect();
        themeObserver.disconnect();
        searchResults.dispose();
        data.dispose();
        inputBuffer.dispose();
        outputBuffer.detach();
        renderer.dispose();
        term.dispose();
        terminal.current = null;
        fit.current = null;
        search.current = null;
        input.current = null;
      };
      if (cancelled) cleanup();
    };

    let cleanup: (() => void) | undefined;
    void start();
    return () => {
      cancelled = true;
      cleanup?.();
      outputBuffer.detach();
    };
  }, []);

  return <div className="terminal-view relative flex h-full min-h-0 w-full flex-col">
    <TerminalToolbar toolbarHost={toolbarHost} searchOpen={searchOpen} query={query} result={searchResult} clipboardError={clipboardError} onOpenSearch={() => setSearchOpen(true)} onCloseSearch={closeSearch} onQueryChange={(value) => { setQuery(value); findNext(value); }} onFindNext={() => findNext()} onFindPrevious={findPrevious} onCopy={() => void copySelection()} onPaste={() => void paste()} />
    <div
      ref={container}
      className="terminal-xterm-host min-h-0 w-full flex-1"
      onContextMenu={(event) => {
        event.preventDefault();
        void paste();
      }}
    />
    <MobileExtraKeys disabled={!sessionId} onInput={sendExtraKey} />
  </div>;
});
