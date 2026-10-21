// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { emptyPythonDraft, validatePythonDraft, validatePythonStep } from "./python-project-draft";
import { PythonProjectPanel } from "./PythonProjectPanel";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async (command) => {
  if (command === "plugin_workspace_entries") return [];
  if (command === "deployment_python_status") return { success: true, output: "inactive" };
  return { success: true, output: "ok" };
}) }));

let root: Root;
let container: HTMLDivElement;

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await i18n.changeLanguage("zh-CN");
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  vi.unstubAllGlobals();
});

describe("Python project draft validation", () => {
  it("accepts a gunicorn asgi draft", () => {
    const draft = {
      ...emptyPythonDraft(),
      name: "demo",
      projectPath: "/srv/demo",
      application: "main:app",
      startup: "gunicorn" as const,
      protocol: "asgi" as const,
    };
    expect(validatePythonDraft(draft)).toBeNull();
  });

  it("rejects uwsgi with asgi", () => {
    const draft = {
      ...emptyPythonDraft(),
      name: "demo",
      projectPath: "/srv/demo",
      application: "main:app",
      startup: "uwsgi" as const,
      protocol: "asgi" as const,
    };
    expect(validatePythonDraft(draft)).toBe("deployment.python.validation.uwsgiAsgi");
  });

  it("accepts command startup with relative command and no port", () => {
    const draft = {
      ...emptyPythonDraft(),
      name: "demo",
      projectPath: "/srv/demo",
      startup: "command" as const,
      startupCommand: "app.py --host 127.0.0.1",
      port: "",
      application: "",
      entryFile: "",
      requirementsPath: "",
    };
    expect(validatePythonDraft(draft)).toBeNull();
  });

  it("rejects unsafe command startup", () => {
    const draft = {
      ...emptyPythonDraft(),
      name: "demo",
      projectPath: "/srv/demo",
      startup: "command" as const,
      startupCommand: "app.py; rm -rf /",
    };
    expect(validatePythonDraft(draft)).toBe("deployment.python.validation.startupCommand");
  });

  it("allows an incomplete draft on step 1 and requires a remote on the git step", () => {
    const draft = {
      ...emptyPythonDraft(),
      name: "demo",
      projectPath: "/srv/demo",
      setupStep: 1 as const,
    };
    expect(validatePythonStep(draft, 1)).toBeNull();
    expect(validatePythonStep({ ...draft, sourceKind: "gitClone", remoteUrl: "" }, 2)).toBe("deployment.validation.remoteUrl");
    expect(validatePythonStep({ ...draft, sourceKind: "existingPath", remoteUrl: "" }, 2)).toBeNull();
  });

  it("rejects an invalid custom venv path on the environment step", () => {
    const draft = {
      ...emptyPythonDraft(),
      name: "demo",
      projectPath: "/srv/demo",
      useExistingVenv: true,
      venvPath: "relative/venv",
    };
    expect(validatePythonStep(draft, 3)).toBe("deployment.python.validation.venvPath");
  });
});

describe("PythonProjectPanel", () => {
  it("renders add action and empty state", async () => {
    await act(async () => {
      root.render(
        <PythonProjectPanel
          sessionId="session-1"
          profileId="profile-1"
          apps={[]}
          onChanged={async () => undefined}
        />,
      );
    });
    expect(container.textContent).toContain(i18n.t("deployment.python.emptyTitle"));
    expect(container.textContent).toContain(i18n.t("deployment.python.emptyHint"));
    expect(container.querySelectorAll("button").length).toBe(1);
  });

  it("disables deploy when the session is disconnected", async () => {
    const apps = [{
      id: "app-1",
      profileId: "profile-1",
      name: "demo",
      repositoryPath: "/srv/demo",
      remoteUrl: "",
      branch: "main",
      build: "pip" as const,
      restart: { kind: "systemd" as const, service: "runory-py-demo.service" },
      projectKind: "python" as const,
      python: {
        port: 8000,
        pythonBin: "python3",
        startup: "gunicorn" as const,
        entryFile: "main.py",
        protocol: "asgi" as const,
        application: "main:app",
        runUser: "www",
        requirementsPath: "requirements.txt",
        startupCommand: "",
      },
      setupStep: 4,
      setupStatus: "ready" as const,
      sourceKind: "existingPath" as const,
      venvPath: "",
      updatedAtEpochSeconds: 1,
    }];
    await act(async () => {
      root.render(
        <PythonProjectPanel
          sessionId={null}
          profileId="profile-1"
          apps={apps}
          onChanged={async () => undefined}
        />,
      );
    });
    const deploy = Array.from(container.querySelectorAll("button")).find((button) =>
      button.textContent?.includes(i18n.t("deployment.python.deploy")),
    );
    expect(deploy).toBeTruthy();
    expect(deploy?.hasAttribute("disabled")).toBe(true);
  });
});
