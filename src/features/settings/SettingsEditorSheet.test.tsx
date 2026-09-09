// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { SettingsEditorSheet } from "./SettingsEditorSheet";

let root: Root;
let container: HTMLDivElement;

beforeEach(async () => {
  await i18n.changeLanguage("zh-CN");
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  vi.unstubAllGlobals();
});

describe("settings editor sheet", () => {
  it("exposes dialog semantics and closes with Escape", async () => {
    const close = vi.fn();
    await act(async () => {
      root.render(<SettingsEditorSheet title="编辑设置" description="说明" onClose={close}><input autoFocus aria-label="名称" /></SettingsEditorSheet>);
      await Promise.resolve();
    });

    const dialog = container.querySelector('[role="dialog"]');
    expect(dialog?.getAttribute("aria-modal")).toBe("true");
    expect(dialog?.getAttribute("aria-labelledby")).toBeTruthy();
    expect(document.activeElement).toBe(container.querySelector('[aria-label="名称"]'));

    await act(async () => { document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })); });
    expect(close).toHaveBeenCalledOnce();
  });

  it("keeps focus inside and blocks dismissal while saving", async () => {
    const close = vi.fn();
    await act(async () => {
      root.render(<SettingsEditorSheet title="编辑设置" closeDisabled onClose={close}><button type="button">首项</button><button type="button">末项</button></SettingsEditorSheet>);
      await Promise.resolve();
    });

    const buttons = Array.from(container.querySelectorAll<HTMLButtonElement>("aside button:not([disabled])"));
    buttons.at(-1)?.focus();
    await act(async () => { document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true })); });
    expect(document.activeElement).toBe(buttons[0]);

    await act(async () => { document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })); });
    expect(close).not.toHaveBeenCalled();
  });
});
