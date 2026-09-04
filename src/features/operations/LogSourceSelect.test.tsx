// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { act, type ComponentProps } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import i18n from "../../i18n";
import { OperationsView } from "./OperationsView";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue({ output: "", success: true }) }));
// jsdom has no layout engine for Floating UI's popper collision loop. Use
// Radix's item-aligned positioning here; browser checks cover the real popper.
vi.mock("../../components/ui/select", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../components/ui/select")>();
  return { ...actual, SelectContent: (props: ComponentProps<typeof actual.SelectContent>) => <actual.SelectContent {...props} position="item-aligned" /> };
});

const sources = ["system", "auth", "nginx-access", "nginx-error", "docker", "pm2", "service"] as const;
let container: HTMLDivElement;
let root: Root;

function getElement<T extends HTMLElement>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`Missing element: ${selector}`);
  return element;
}

async function click(element: HTMLElement) {
  await act(async () => { element.click(); });
}

function button(key: string, scope: ParentNode = container): HTMLButtonElement {
  const element = Array.from(scope.querySelectorAll<HTMLButtonElement>("button")).find((item) => item.textContent === i18n.t(key));
  if (!element) throw new Error(`Missing button: ${key}`);
  return element;
}

async function navigate(section: "docker" | "pm2" | "nginx" | "logs") {
  await click(button(`operations.${section}`, getElement("nav")));
}

async function press(element: HTMLElement, key: string) {
  await act(async () => {
    element.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    // Radix defers focus movement until after the keyboard event.
    await new Promise((resolve) => setTimeout(resolve, 10));
  });
}

async function mountLogs(active = false) {
  await act(async () => { root.render(<OperationsView sessionId="test-session" active={active} />); });
  const tab = Array.from(container.querySelectorAll("nav button")).find((button) => button.textContent === i18n.t("operations.logs"));
  if (!(tab instanceof HTMLElement)) throw new Error("Missing logs tab");
  await click(tab);
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  // Layout scrolling is absent in jsdom; keep the real Radix event handlers.
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(180);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(36);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, top: 0, left: 0, right: 180, bottom: 36, width: 180, height: 36, toJSON() {} });
  const layoutStyle = document.createElement("div").style;
  layoutStyle.cssText = "position: static; display: block; visibility: visible; direction: ltr; width: 180px; height: 36px; transform: none; filter: none; perspective: none;";
  vi.spyOn(window, "getComputedStyle").mockReturnValue(layoutStyle);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  vi.mocked(invoke).mockReset().mockImplementation(async (command) => {
    if (command === "docker_list" || command === "pm2_list") return [];
    return { output: "", success: true };
  });
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  document.documentElement.classList.remove("dark");
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe.each(["en-US", "zh-CN"])("log source select (%s)", (language) => {
  it.each(["light", "dark"])("keeps all sources readable and preserves log filtering in %s mode", async (theme) => {
    await i18n.changeLanguage(language);
    document.documentElement.classList.toggle("dark", theme === "dark");
    await mountLogs();
    const trigger = getElement<HTMLButtonElement>('[role="combobox"]');
    expect(trigger.getAttribute("aria-label")).toBe(i18n.t("operations.logSource"));
    expect(trigger.textContent).toContain(i18n.t("operations.log.system"));
    expect(trigger.classList.contains("bg-surface")).toBe(true);
    expect(trigger.classList.contains("text-foreground")).toBe(true);

    for (const source of sources) {
      await press(trigger, "ArrowDown");
      const popup = getElement('[role="listbox"]');
      expect(container.contains(popup)).toBe(false);
      expect(popup.classList.contains("bg-surface")).toBe(true);
      expect(popup.classList.contains("text-foreground")).toBe(true);
      const options = Array.from(popup.querySelectorAll<HTMLElement>('[role="option"]'));
      expect(options.map((option) => option.textContent)).toEqual(sources.map((value) => i18n.t(`operations.log.${value}`)));
      const option = options[sources.indexOf(source)];
      expect(option.className).toContain("data-[highlighted]:bg-[hsl(var(--elevated))]");
      await click(option);
      expect(document.querySelector('[role="listbox"]')).toBeNull();
      expect(trigger.textContent).toContain(i18n.t(`operations.log.${source}`));
      const target = getElement<HTMLInputElement>('input:not([type="number"])');
      expect(target.disabled).toBe(!["docker", "pm2", "service"].includes(source));
    }
    expect(invoke).not.toHaveBeenCalled();
    const target = getElement<HTMLInputElement>('input:not([type="number"])');
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(target, "api");
      target.dispatchEvent(new Event("input", { bubbles: true }));
    });
    const readButton = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === i18n.t("operations.readLogs"));
    if (!readButton) throw new Error("Missing read button");
    await click(readButton);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("logs_read", { request: { sessionId: "test-session", source: "service", target: "api", lines: 200 } });
  });

  it("supports arrow navigation, Enter selection and Escape cancellation", async () => {
    await i18n.changeLanguage(language);
    await mountLogs();
    const trigger = getElement<HTMLButtonElement>('[role="combobox"]');
    await press(trigger, "ArrowDown");
    expect(document.activeElement?.getAttribute("role")).toBe("option");
    await press(document.activeElement as HTMLElement, "ArrowDown");
    await press(document.activeElement as HTMLElement, "Enter");
    expect(trigger.textContent).toContain(i18n.t("operations.log.auth"));
    expect(document.activeElement).toBe(trigger);
    await press(trigger, "Enter");
    expect(getElement('[role="option"][data-state="checked"]').textContent).toBe(i18n.t("operations.log.auth"));
    await press(document.activeElement as HTMLElement, "ArrowDown");
    await press(document.activeElement as HTMLElement, "Escape");
    expect(document.querySelector('[role="listbox"]')).toBeNull();
    expect(trigger.textContent).toContain(i18n.t("operations.log.auth"));
    expect(document.activeElement).toBe(trigger);
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("log output layout", () => {
  it("fills the available height before and after reading, with scrolling inside the output", async () => {
    await mountLogs();
    const output = getElement("pre");
    expect(output.textContent).toBe("");
    expect(output.classList.contains("flex-1")).toBe(true);
    expect(output.classList.contains("min-h-0")).toBe(true);
    expect(output.classList.contains("overflow-auto")).toBe(true);
    expect(output.classList.contains("max-h-96")).toBe(false);
    expect(output.parentElement?.className).toContain("flex flex-col overflow-hidden");
    expect(output.previousElementSibling?.classList.contains("shrink-0")).toBe(true);

    vi.mocked(invoke).mockResolvedValueOnce({ output: "line\n".repeat(200), success: true });
    const readButton = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === i18n.t("operations.readLogs"));
    if (!readButton) throw new Error("Missing read button");
    await click(readButton);
    expect(output.textContent).toBe("line\n".repeat(200));
    expect(output.classList.contains("flex-1")).toBe(true);

    const nginxTab = Array.from(container.querySelectorAll<HTMLButtonElement>("nav button")).find((button) => button.textContent === i18n.t("operations.nginx"));
    if (!nginxTab) throw new Error("Missing Nginx tab");
    await click(nginxTab);
    expect(container.querySelector("pre")).toBeNull();
    await navigate("logs");
    expect(getElement("pre").textContent).toBe("line\n".repeat(200));
    expect(getElement("pre").classList.contains("flex-1")).toBe(true);
  });
});

describe("operation output isolation", () => {
  it.each(["docker", "pm2", "nginx"] as const)("does not show loaded logs on the %s tab", async (section) => {
    await mountLogs(true);
    vi.mocked(invoke).mockResolvedValueOnce({ output: "LOGS_ONLY", success: true });
    await click(button("operations.readLogs"));
    expect(getElement("pre").textContent).toBe("LOGS_ONLY");
    await navigate(section);
    expect(container.querySelector("pre")).toBeNull();
    expect(container.textContent).not.toContain("LOGS_ONLY");
    await navigate("logs");
    expect(getElement("pre").textContent).toBe("LOGS_ONLY");
  });

  it("keeps a late log response on Logs after switching tabs", async () => {
    await mountLogs(true);
    let resolveRead!: (value: { output: string; success: boolean }) => void;
    vi.mocked(invoke).mockReturnValueOnce(new Promise((resolve) => { resolveRead = resolve; }));
    await click(button("operations.readLogs"));
    await navigate("nginx");
    await act(async () => { resolveRead({ output: "LATE_LOGS", success: true }); });
    expect(container.querySelector("pre")).toBeNull();
    await navigate("logs");
    expect(getElement("pre").textContent).toBe("LATE_LOGS");
  });

  it("does not show a late log error on another tab", async () => {
    await mountLogs();
    let rejectRead!: (error: Error) => void;
    vi.mocked(invoke).mockReturnValueOnce(new Promise((_resolve, reject) => { rejectRead = reject; }));
    await click(button("operations.readLogs"));
    await navigate("nginx");
    await act(async () => { rejectRead(new Error("test failure")); });
    expect(container.textContent).not.toContain(i18n.t("operations.unsupported"));
    await navigate("logs");
    expect(container.textContent).toContain(i18n.t("operations.unsupported"));
  });

  it("keeps Nginx action output separate from existing logs", async () => {
    await mountLogs();
    vi.mocked(invoke).mockResolvedValueOnce({ output: "LOGS_ONLY", success: true });
    await click(button("operations.readLogs"));
    await navigate("nginx");
    await click(button("operations.action.test"));
    vi.mocked(invoke).mockResolvedValueOnce({ output: "NGINX_ONLY", success: true });
    await click(button("operations.action.test", getElement('[role="dialog"]')));
    expect(getElement("pre").textContent).toBe("NGINX_ONLY");
    expect(getElement("pre").classList.contains("max-h-96")).toBe(true);
    await navigate("logs");
    expect(getElement("pre").textContent).toBe("LOGS_ONLY");
    await navigate("docker");
    expect(container.querySelector("pre")).toBeNull();
    await navigate("nginx");
    expect(getElement("pre").textContent).toBe("NGINX_ONLY");
  });
});

// Check the actual theme token pairs used for normal and highlighted options.
describe("select theme contrast", () => {
  const css = readFileSync("src/styles.css", "utf8");
  function luminance(block: string, name: string) {
    const value = block.match(new RegExp(`--${name}:\\s*([\\d.]+) ([\\d.]+)% ([\\d.]+)%`));
    if (!value) throw new Error(`Missing theme token: ${name}`);
    const h = Number(value[1]) / 30;
    const s = Number(value[2]) / 100;
    const l = Number(value[3]) / 100;
    const a = s * Math.min(l, 1 - l);
    const rgb = [0, 8, 4].map((n) => {
      const k = (n + h) % 12;
      const c = l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
      return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    });
    return rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
  }
  it.each([":root", ".dark"])("has at least 4.5:1 text contrast in %s", (selector) => {
    const block = css.slice(css.indexOf(`${selector} {`)).split("}")[0];
    const foreground = luminance(block, "foreground");
    for (const token of ["surface", "elevated"]) {
      const background = luminance(block, token);
      expect((Math.max(foreground, background) + 0.05) / (Math.min(foreground, background) + 0.05)).toBeGreaterThanOrEqual(4.5);
    }
  });
});
