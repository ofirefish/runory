// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { suppressTerminalAccessibility, terminalCanvasIsHealthy } from "./terminal-renderer";

describe("terminal renderer helpers", () => {
  it("removes accessibility and message layers", () => {
    const host = document.createElement("div");
    host.innerHTML = `
      <div class="xterm">
        <div class="xterm-accessibility"><div class="xterm-accessibility-tree">plain</div></div>
        <div class="xterm-message">bell</div>
        <div class="xterm-screen"><canvas width="10" height="10"></canvas></div>
      </div>
    `;
    suppressTerminalAccessibility(host);
    expect(host.querySelector(".xterm-accessibility")).toBeNull();
    expect(host.querySelector(".xterm-message")).toBeNull();
    expect(host.querySelector("canvas")).not.toBeNull();
  });

  it("reports healthy canvas when client box is non-zero", () => {
    const host = document.createElement("div");
    const canvas = document.createElement("canvas");
    Object.defineProperty(canvas, "clientWidth", { value: 120 });
    Object.defineProperty(canvas, "clientHeight", { value: 40 });
    const screen = document.createElement("div");
    screen.className = "xterm-screen";
    screen.appendChild(canvas);
    host.appendChild(screen);
    expect(terminalCanvasIsHealthy(host)).toBe(true);
  });

  it("reports unhealthy canvas when client box is zero", () => {
    const host = document.createElement("div");
    const canvas = document.createElement("canvas");
    vi.spyOn(canvas, "clientWidth", "get").mockReturnValue(0);
    vi.spyOn(canvas, "clientHeight", "get").mockReturnValue(0);
    const screen = document.createElement("div");
    screen.className = "xterm-screen";
    screen.appendChild(canvas);
    host.appendChild(screen);
    expect(terminalCanvasIsHealthy(host)).toBe(false);
  });
});
