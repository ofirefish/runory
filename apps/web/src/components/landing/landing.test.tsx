import { act, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WorkspaceTour } from "./workspace-tour";
import { AgentWalkthrough, DownloadPicker, LandingHeader } from "./landing-interactions";
import { getLandingCopy } from "@/lib/landing-copy";

vi.mock("next/link", () => ({ default: ({ children, href, ...props }: { children: ReactNode; href: string }) => <a href={href} {...props}>{children}</a> }));
vi.mock("@/components/brand-mark", () => ({ BrandMark: () => <span /> }));

let container: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
});
async function render(node: ReactNode) { await act(async () => root.render(node)); }
async function click(element: Element | null) {
  expect(element).not.toBeNull();
  if (element instanceof HTMLAnchorElement) element.addEventListener("click", event => event.preventDefault(), { once: true });
  await act(async () => element?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true })));
}

describe.each(["zh-CN", "en-US"] as const)("landing interactions: %s", locale => {
  const t = getLandingCopy(locale);
  it("switches workspace content with keyboard focus and matching panel labels", async () => {
    await render(<WorkspaceTour copy={t.demo} />);
    const tabs = container.querySelectorAll<HTMLButtonElement>('[role="tab"]');
    expect(container.textContent).toContain(t.demo.caption);
    await act(async () => tabs[0].dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })));
    expect(document.activeElement).toBe(tabs[1]);
    expect(tabs[1].getAttribute("aria-selected")).toBe("true");
    expect(container.querySelector('[role="tabpanel"]')?.getAttribute("aria-labelledby")).toBe(tabs[1].id);
    expect(container.textContent).toContain("docker-compose.yml");
    expect(container.textContent).not.toContain("docker ps");
    await click(tabs[2]);
    expect(container.textContent).toContain(t.demo.metrics[0]);
    await click(tabs[3]);
    expect(container.textContent).toContain(t.demo.deploymentSteps[3]);
  });
  it("explains each AI stage without offering an execution control", async () => {
    await render(<AgentWalkthrough copy={t.agent} />);
    const steps = container.querySelectorAll("button");
    await click(steps[1]);
    expect(container.querySelector('[aria-live="polite"]')?.textContent).toContain(t.agent.summaries[1]);
    await click(steps[2]);
    expect(container.textContent).toContain(t.agent.result);
    expect(container.textContent).toContain(t.agent.note);
    expect(steps).toHaveLength(3);
  });
  it("changes platform guidance and keeps download destinations honest", async () => {
    const downloads = {
      version: "v0.1.0",
      releasePageUrl: "https://github.com/ofirefish/runory/releases",
      assets: [
        { id: "windows", os: "windows" as const, arch: "x64" as const, labelKey: "windows" as const, format: ".msi", href: "https://example.com/win.msi", direct: true },
        { id: "macos-apple", os: "macos" as const, arch: "arm64" as const, labelKey: "macosApple" as const, format: ".dmg", href: "https://example.com/mac-arm.dmg", direct: true },
        { id: "macos-intel", os: "macos" as const, arch: "x64" as const, labelKey: "macosIntel" as const, format: ".dmg", href: "https://example.com/mac-x64.dmg", direct: true },
        { id: "linux", os: "linux" as const, arch: "x64" as const, labelKey: "linux" as const, format: ".AppImage", href: "https://example.com/linux.AppImage", direct: true },
      ],
    };
    await render(<DownloadPicker copy={t.downloads} downloads={downloads} />);
    const tabs = container.querySelectorAll<HTMLButtonElement>('[role="tab"]');
    await act(async () => tabs[0].dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true })));
    expect(tabs[2].getAttribute("aria-selected")).toBe("true");
    expect(container.textContent).toContain(t.downloads.packages[2]);
    expect(container.querySelector("a")?.getAttribute("href")).toBe("https://example.com/linux.AppImage");
    await click(tabs[1]);
    const links = [...container.querySelectorAll("a")].map(anchor => anchor.getAttribute("href"));
    expect(links).toContain("https://example.com/mac-arm.dmg");
    expect(links).toContain("https://example.com/mac-x64.dmg");
  });
  it("falls back to the releases page when assets are unavailable", async () => {
    await render(<DownloadPicker copy={t.downloads} />);
    expect(container.querySelector("a")?.getAttribute("href")).toBe("https://github.com/ofirefish/runory/releases");
  });
  it("closes the mobile menu on Escape and returns focus to its trigger", async () => {
    await render(<LandingHeader locale={locale} copy={t.nav} />);
    const trigger = container.querySelector<HTMLButtonElement>('[aria-controls="mobile-navigation"]');
    await click(trigger);
    expect(container.querySelector("#mobile-navigation")).not.toBeNull();
    await act(async () => container.querySelector("header")?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
    expect(container.querySelector("#mobile-navigation")).toBeNull();
    expect(document.activeElement).toBe(trigger);
    await click(trigger);
    await click(container.querySelector(`#mobile-navigation a[href="/${locale}/product"]`));
    expect(container.querySelector("#mobile-navigation")).toBeNull();
  });
});

it("keeps both dictionaries complete, including list entries", () => {
  function shape(value: unknown): unknown {
    if (typeof value === "string") { expect(value.trim().length).toBeGreaterThan(0); return "string"; }
    if (Array.isArray(value)) return value.map(shape);
    return Object.fromEntries(Object.entries(value as Record<string, unknown>).map(([key, item]) => [key, shape(item)]));
  }
  expect(shape(getLandingCopy("en-US"))).toEqual(shape(getLandingCopy("zh-CN")));
});
