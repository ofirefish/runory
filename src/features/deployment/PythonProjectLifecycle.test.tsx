// @vitest-environment jsdom
import { invoke } from "@tauri-apps/api/core";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import type { DeploymentApp } from "../../types/infrastructure";
import { PythonProjectPanel } from "./PythonProjectPanel";
import { draftFromPythonApp, pythonConfigFromDraft, validatePythonDraft } from "./python-project-draft";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const app: DeploymentApp = {
  id: "python-app", profileId: "profile", name: "demo", repositoryPath: "/srv/demo",
  remoteUrl: "", branch: "main", build: "pip", projectKind: "python",
  restart: { kind: "systemd", service: "runory-py-demo.service" },
  setupStep: 4, setupStatus: "provisioned", updatedAtEpochSeconds: 0,
  python: { port: 8000, pythonBin: "python3", startup: "gunicorn", protocol: "wsgi", application: "app:app", entryFile: "app.py", runUser: "www", requirementsPath: "" },
};
let root: Root;
let container: HTMLDivElement;

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "deployment_python_status") return { success: true, output: "inactive" };
    return { success: true, output: "active" };
  });
  await i18n.changeLanguage("en-US");
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
});
async function render(project = app, sessionId: string | null = "session") {
  await act(async () => root.render(<PythonProjectPanel sessionId={sessionId} profileId="profile" apps={[project]} onChanged={async () => undefined} />));
}
function button(key: string) {
  const found = [...document.querySelectorAll("button")].find((element) => element.textContent === i18n.t(key));
  if (!found) throw new Error(`Missing button ${key}`);
  return found;
}

describe("Python project lifecycle", () => {
  it("requires confirmation, binds control to app identity and refreshes status", async () => {
    await render();
    await act(async () => button("deployment.python.action.start").click());
    expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "deployment_python_control")).toBe(false);
    await act(async () => button("deployment.confirm").click());
    expect(invoke).toHaveBeenCalledWith("deployment_python_control", { request: { sessionId: "session", appId: app.id, action: "start" } });
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "deployment_python_status")).toHaveLength(2);
  });

  it("cancel does not send a remote operation", async () => {
    await render();
    await act(async () => button("deployment.python.action.stop").click());
    await act(async () => button("common.cancel").click());
    expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "deployment_python_control")).toBe(false);
  });

  it("disables control while deploying or disconnected", async () => {
    await render({ ...app, setupStatus: "provisioning" });
    expect(button("deployment.python.action.restart").disabled).toBe(true);
    await render(app, null);
    expect(button("deployment.python.action.start").disabled).toBe(true);
    expect(button("deployment.python.logs").disabled).toBe(true);
  });

  it("reads bounded service logs as plain text", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => ({ success: true, output: command === "logs_read" ? "<script>alert(1)</script>" : "inactive" }));
    await render();
    await act(async () => button("deployment.python.logs").click());
    expect(invoke).toHaveBeenCalledWith("logs_read", { request: { sessionId: "session", source: "service", target: "runory-py-demo.service", lines: 200 } });
    expect(container.querySelector("pre")?.textContent).toBe("<script>alert(1)</script>");
    expect(container.querySelector("script")).toBeNull();
  });

  it("does not report an empty successful status response as running", async () => {
    vi.mocked(invoke).mockResolvedValue({ success: true, output: "" });
    await render();
    expect(container.textContent).toContain(i18n.t("deployment.python.status.unknown"));
  });

  it("keeps failed control visible without a success message", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => ({ success: command !== "deployment_python_control", output: "inactive" }));
    await render();
    await act(async () => button("deployment.python.action.restart").click());
    await act(async () => button("deployment.confirm").click());
    expect(document.body.textContent).toContain(i18n.t("deployment.error"));
    expect(document.body.textContent).not.toContain(i18n.t("deployment.python.controlSuccess"));
  });
});

describe("Python deployment options", () => {
  it("preserves legacy defaults and round-trips concurrency and boot policy", () => {
    const draft = draftFromPythonApp(app);
    expect(draft.autoStart).toBe(true);
    const updated = { ...draft, processes: "4", threads: "2", autoStart: false };
    expect(validatePythonDraft(updated)).toBeNull();
    expect(pythonConfigFromDraft(updated)).toMatchObject({ processes: 4, threads: 2, autoStart: false });
    for (const processes of ["0", "65", "1.5", "NaN", ""]) {
      expect(validatePythonDraft({ ...updated, processes })).toBe("deployment.python.validation.concurrency");
    }
  });

  it("validates uWSGI callable independently from Gunicorn application", () => {
    const draft = { ...draftFromPythonApp(app), startup: "uwsgi" as const, application: "app" };
    expect(validatePythonDraft(draft)).toBeNull();
    expect(validatePythonDraft({ ...draft, application: "app:app" })).toBe("deployment.python.validation.callable");
  });
});
