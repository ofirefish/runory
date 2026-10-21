// @vitest-environment jsdom
import { act, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { PythonProjectWizard } from "./PythonProjectWizard";
import { emptyPythonDraft } from "./python-project-draft";

vi.mock("../../lib/tauri/ssh", () => ({
  openSftp: vi.fn(async () => ({ path: "/srv" })),
  listDirectory: vi.fn(async () => ({ path: "/srv", entries: [{ name: "project", path: "/srv/project", kind: "directory", modified: null }] })),
  createRemoteDirectory: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

let root: Root;
let container: HTMLDivElement;
const onClose = vi.fn();
function NewProject() {
  const [open, setOpen] = useState(true);
  return open ? <PythonProjectWizard title={i18n.t("deployment.python.addProject")} initial={{ ...emptyPythonDraft(), name: "demo", projectPath: "/srv" }} sessionId="session" onClose={() => { onClose(); setOpen(false); }} onSave={async () => undefined} onFinish={async () => undefined} /> : null;
}
beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  onClose.mockClear();
  vi.mocked(invoke).mockReset();
  await i18n.changeLanguage("zh-CN");
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root.render(<NewProject />));
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
});
afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
});
async function click(button: HTMLElement) {
  await act(async () => {
    button.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, button: 0 }));
    button.focus();
    button.dispatchEvent(new MouseEvent("pointerup", { bubbles: true, button: 0 }));
    button.click();
  });
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
}
function findButton(label: string) {
  const button = [...document.querySelectorAll<HTMLButtonElement>("button")].find((item) => item.textContent === label || item.getAttribute("aria-label") === label);
  if (!button) throw new Error(`Missing button: ${label}`);
  return button;
}

it("selects a remote path without closing or clearing the new project wizard", async () => {
  await click(findButton(i18n.t("remoteFolderPicker.browse")));
  await click(findButton("project"));
  await click(findButton(i18n.t("remoteFolderPicker.confirm")));
  expect(onClose).not.toHaveBeenCalled();
  expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1);
  expect([...document.querySelectorAll<HTMLInputElement>("input")].map((input) => input.value)).toEqual(expect.arrayContaining(["demo", "/srv/project"]));
});

it("cancels only the folder picker and preserves the draft", async () => {
  await click(findButton(i18n.t("remoteFolderPicker.browse")));
  const cancel = [...document.querySelectorAll<HTMLButtonElement>("button")].filter((button) => button.textContent === i18n.t("common.cancel")).at(-1);
  expect(cancel).toBeTruthy();
  await click(cancel!);
  expect(onClose).not.toHaveBeenCalled();
  expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1);
  expect([...document.querySelectorAll<HTMLInputElement>("input")].map((input) => input.value)).toEqual(expect.arrayContaining(["demo", "/srv"]));
});

it("Escape closes only the topmost picker, then can close the wizard", async () => {
  await click(findButton(i18n.t("remoteFolderPicker.browse")));
  await act(async () => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  });
  expect(onClose).not.toHaveBeenCalled();
  expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1);
  await act(async () => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  });
  expect(onClose).toHaveBeenCalledOnce();
});

it.each([
  ["DEPLOYMENT_GIT_TARGET_NOT_EMPTY", "deployment.python.gitTargetNotEmpty"],
  ["DEPLOYMENT_GIT_PATH_DENIED", "deployment.python.gitPathDenied"],
])("explains Git directory failure %s without advancing the wizard", async (code, key) => {
  vi.mocked(invoke).mockRejectedValue({ code });
  const onSave = vi.fn();
  await act(async () => root.render(<PythonProjectWizard
    title={i18n.t("deployment.python.addProject")}
    initial={{ ...emptyPythonDraft(), name: "demo", projectPath: "/home/user", setupStep: 2, sourceKind: "gitClone", remoteUrl: "https://example.com/demo.git" }}
    sessionId="session" onClose={onClose} onSave={onSave} onFinish={async () => undefined}
  />));
  await click(findButton(i18n.t("deployment.python.next")));
  expect(document.body.textContent).toContain(i18n.t(key));
  expect(document.body.textContent).not.toContain(i18n.t("deployment.validation.invalid"));
  expect(onSave).not.toHaveBeenCalled();
  expect(onClose).not.toHaveBeenCalled();
});
