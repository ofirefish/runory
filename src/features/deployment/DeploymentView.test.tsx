// @vitest-environment jsdom
import { act, type ComponentProps } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import i18n from "../../i18n";
import { DeploymentView } from "./DeploymentView";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
// Exercise real Radix selection and focus handling without jsdom's unsupported
// Floating UI collision/layout loop. Popper appearance is checked in a browser.
vi.mock("../../components/ui/select", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../components/ui/select")>();
  return { ...actual, SelectContent: (props: ComponentProps<typeof actual.SelectContent>) => <actual.SelectContent {...props} position="item-aligned" /> };
});

let root: Root;
let container: HTMLDivElement;

function field<T extends HTMLElement>(key: string): T {
  const label = Array.from(container.querySelectorAll("label")).find((item) => item.textContent === i18n.t(key));
  const control = label && document.getElementById(label.htmlFor);
  if (!control) throw new Error(`Missing field: ${key}`);
  expect(label?.dataset.slot).toBe("label");
  return control as T;
}

async function clickButton(key: string, scope: ParentNode = document) {
  const button = Array.from(scope.querySelectorAll<HTMLButtonElement>("button")).find((item) => item.textContent === i18n.t(key));
  if (!button) throw new Error(`Missing button: ${key}`);
  await act(async () => { button.click(); });
}

async function navigate(section: string) {
  await clickButton(`deployment.${section}`, container.querySelector("nav")!);
}

async function fill(key: string, value: string) {
  const element = field<HTMLInputElement | HTMLTextAreaElement>(key);
  const prototype = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  await act(async () => {
    Object.getOwnPropertyDescriptor(prototype, "value")?.set?.call(element, value);
    element.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

async function select(key: string, optionKey: string, expectedOptions?: string[]) {
  const trigger = field<HTMLButtonElement>(key);
  expect(trigger.getAttribute("role")).toBe("combobox");
  await act(async () => { trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })); });
  const options = Array.from(document.querySelectorAll<HTMLElement>('[role="option"]'));
  if (expectedOptions) expect(options.map((item) => item.textContent)).toEqual(expectedOptions.map((item) => i18n.t(item)));
  const option = options.find((item) => item.textContent === i18n.t(optionKey));
  if (!option) throw new Error(`Missing option: ${optionKey}`);
  await act(async () => { option.click(); });
  expect(trigger.textContent).toContain(i18n.t(optionKey));
}

function writes() {
  return vi.mocked(invoke).mock.calls.filter(([command]) => !["deployment_cron_list", "deployment_history"].includes(command));
}

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(180);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(36);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, top: 0, left: 0, right: 180, bottom: 36, width: 180, height: 36, toJSON() {} });
  const layoutStyle = document.createElement("div").style;
  layoutStyle.cssText = "position: static; display: block; visibility: visible; direction: ltr; width: 180px; height: 36px; transform: none; filter: none; perspective: none;";
  vi.spyOn(window, "getComputedStyle").mockReturnValue(layoutStyle);
  vi.mocked(invoke).mockReset().mockImplementation(async (command) => {
    if (["deployment_cron_list", "deployment_history"].includes(command)) return [];
    return { output: "ok", success: true };
  });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  document.documentElement.classList.remove("dark");
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe.each(["en-US", "zh-CN"])("Deployment forms (%s)", (language) => {
  beforeEach(async () => {
    await i18n.changeLanguage(language);
    await act(async () => { root.render(<DeploymentView sessionId="test-session" profileId="test-profile" />); });
  });

  it.each(["light", "dark"])("uses shared, labelled controls across every form in %s mode", async (theme) => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    for (const [section, inputCount, selectCount, textareaCount] of [
      ["git", 3, 0, 0], ["deploy", 2, 2, 0], ["environment", 1, 0, 1], ["ssl", 3, 0, 0], ["backup", 2, 0, 0], ["cron", 2, 2, 0],
    ] as const) {
      await navigate(section);
      expect(container.querySelectorAll("input")).toHaveLength(inputCount);
      expect(container.querySelectorAll('[role="combobox"]')).toHaveLength(selectCount);
      expect(container.querySelectorAll('[data-slot="textarea"]')).toHaveLength(textareaCount);
      expect(container.querySelectorAll("select")).toHaveLength(0);
      const controls = container.querySelectorAll<HTMLElement>('input, textarea, [role="combobox"]');
      const labels = Array.from(container.querySelectorAll('label[data-slot="label"]')) as HTMLLabelElement[];
      for (const control of controls) {
        expect(labels.some((label) => label.htmlFor === control.id && label.textContent)).toBe(true);
        expect(control.className).toContain("text-foreground");
      }
    }
    expect(writes()).toHaveLength(0);
  });

  it("preserves deploy options, conditional restart fields, and confirmation payload", async () => {
    await fill("deployment.repositoryPath", "/srv/demo");
    await fill("deployment.branch", "release");
    await select("deployment.buildPreset", "deployment.build.pnpm", ["none", "npm", "pnpm", "cargo"].map((value) => `deployment.build.${value}`));
    for (const kind of ["systemd", "pm2", "dockerCompose"] as const) {
      await select("deployment.restartTarget", `deployment.restart.${kind}`, ["none", "systemd", "pm2", "dockerCompose"].map((value) => `deployment.restart.${value}`));
      expect(field("deployment.restartName")).toBeTruthy();
    }
    await select("deployment.restartTarget", "deployment.restart.none");
    expect(container.querySelectorAll("input")).toHaveLength(2);
    await select("deployment.restartTarget", "deployment.restart.systemd");
    await fill("deployment.restartName", "demo.service");
    await clickButton("deployment.run");
    expect(writes()).toHaveLength(0);
    await clickButton("common.cancel", document.querySelector('[role="dialog"]')!);
    expect(writes()).toHaveLength(0);
    await clickButton("deployment.run");
    await clickButton("deployment.confirm", document.querySelector('[role="dialog"]')!);
    expect(writes()).toEqual([["deployment_run", { request: { sessionId: "test-session", repositoryPath: "/srv/demo", branch: "release", build: "pnpm", restart: { kind: "systemd", service: "demo.service" } } }]]);
  });

  it("preserves all cron task variants and their conditional fields", async () => {
    await navigate("cron");
    await select("deployment.schedule", "deployment.schedule.weekly", ["hourly", "daily", "weekly"].map((value) => `deployment.schedule.${value}`));
    await select("deployment.cronTask", "deployment.cron.serviceRestart", ["backup", "serviceRestart", "gitPull"].map((value) => `deployment.cron.${value}`));
    expect(container.querySelectorAll("input")).toHaveLength(1);
    await fill("deployment.serviceName", "demo.service");
    await clickButton("deployment.addCron");
    expect(writes()).toHaveLength(0);
    await clickButton("deployment.confirm", document.querySelector('[role="dialog"]')!);
    expect(writes()[0]).toEqual(["deployment_cron_add", { request: { sessionId: "test-session", schedule: "weekly", task: { kind: "serviceRestart", service: "demo.service" } } }]);
    await select("deployment.cronTask", "deployment.cron.gitPull");
    expect(field("deployment.repositoryPath")).toBeTruthy();
    expect(field("deployment.branch")).toBeTruthy();
    await select("deployment.cronTask", "deployment.cron.backup");
    expect(field("deployment.sourcePath")).toBeTruthy();
    expect(field("deployment.destinationDirectory")).toBeTruthy();
  });

  it("keeps environment text transient and clears it only after successful confirmed write", async () => {
    await navigate("environment");
    const textarea = field<HTMLTextAreaElement>("deployment.environmentEntries");
    expect(textarea.dataset.slot).toBe("textarea");
    expect(textarea.getAttribute("spellcheck")).toBe("false");
    expect(document.getElementById(textarea.getAttribute("aria-describedby")!)?.textContent).toBe(i18n.t("deployment.environmentSecurity"));
    await fill("deployment.environmentEntries", "DEMO=one=two\nEMPTY=\n");
    await clickButton("deployment.writeEnvironment");
    expect(writes()).toHaveLength(0);
    await clickButton("common.cancel", document.querySelector('[role="dialog"]')!);
    expect(textarea.value).toContain("DEMO=one=two");
    vi.mocked(invoke).mockResolvedValueOnce({ output: "failed", success: false });
    await clickButton("deployment.writeEnvironment");
    await clickButton("deployment.confirm", document.querySelector('[role="dialog"]')!);
    expect(textarea.value).toContain("DEMO=one=two");
    await clickButton("deployment.writeEnvironment");
    await clickButton("deployment.confirm", document.querySelector('[role="dialog"]')!);
    expect(writes().at(-1)).toEqual(["deployment_environment_write", { request: { sessionId: "test-session", path: "/srv/app/.env", entries: [{ key: "DEMO", value: "one=two" }, { key: "EMPTY", value: "" }] } }]);
    expect(textarea.value).toBe("");
  });
});
