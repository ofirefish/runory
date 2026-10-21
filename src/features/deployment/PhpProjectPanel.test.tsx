// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import type { DeploymentApp } from "../../types/infrastructure";
import { PhpProjectPanel } from "./PhpProjectPanel";
import { draftFromPhpApp, emptyPhpDraft, phpConfigFromDraft, validatePhpDraft, validatePhpStep } from "./php-project-draft";

const invoke = vi.hoisted(() => vi.fn(async (command: string) => {
  if (command === "plugin_workspace_entries") return [];
  if (command === "deployment_python_status") return { success: true, output: "active" };
  return { success: true, output: "ok" };
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

let root: Root;
let container: HTMLDivElement;

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  invoke.mockClear();
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

const readyDraft = () => ({ ...emptyPhpDraft(), name: "shop", projectPath: "/srv/shop", setupStep: 4 as const });

function phpApp(overrides: Partial<DeploymentApp> = {}): DeploymentApp {
  return {
    id: "app-1",
    profileId: "profile-1",
    name: "shop",
    repositoryPath: "/srv/shop",
    remoteUrl: "",
    branch: "main",
    build: "none",
    restart: { kind: "systemd", service: "runory-php-shop.service" },
    projectKind: "php",
    php: phpConfigFromDraft(readyDraft()),
    setupStep: 4,
    setupStatus: "provisioned",
    sourceKind: "existingPath",
    venvPath: "",
    updatedAtEpochSeconds: 1,
    ...overrides,
  };
}

describe("PHP project draft validation", () => {
  it("accepts the default final-step draft", () => {
    expect(validatePhpDraft(readyDraft())).toBeNull();
  });

  it("rejects paths that cannot be written safely into nginx or php-fpm", () => {
    expect(validatePhpStep({ ...readyDraft(), projectPath: "/srv/my shop" }, 1)).toBe("deployment.php.validation.pathChars");
    expect(validatePhpStep({ ...readyDraft(), projectPath: "/srv/$root" }, 1)).toBe("deployment.php.validation.pathChars");
    expect(validatePhpStep({ ...readyDraft(), projectPath: "srv/shop" }, 1)).toBe("deployment.validation.repositoryPath");
    expect(validatePhpStep({ ...readyDraft(), runDir: "../public" }, 3)).toBe("deployment.php.validation.runDir");
    expect(validatePhpStep({ ...readyDraft(), runDir: "/public/" }, 3)).toBeNull();
  });

  it("requires a valid PHP binary, port, domains and limits", () => {
    expect(validatePhpStep({ ...readyDraft(), phpBin: "php8.2" }, 3)).toBeNull();
    expect(validatePhpStep({ ...readyDraft(), phpBin: "python3" }, 3)).toBe("deployment.php.validation.phpBin");
    expect(validatePhpDraft({ ...readyDraft(), port: "0" })).toBe("deployment.php.validation.port");
    expect(validatePhpDraft({ ...readyDraft(), serverName: "example.com localhost" })).toBeNull();
    expect(validatePhpDraft({ ...readyDraft(), serverName: "bad host;" })).toBe("deployment.php.validation.serverName");
    expect(validatePhpDraft({ ...readyDraft(), maxChildren: "0" })).toBe("deployment.php.validation.limits");
    expect(validatePhpDraft({ ...readyDraft(), uploadMaxMb: "4096" })).toBe("deployment.php.validation.limits");
    expect(validatePhpDraft({ ...readyDraft(), runUser: "bad user" })).toBe("deployment.php.validation.runUser");
  });

  it("round-trips a saved project through the draft", () => {
    const draft = { ...readyDraft(), phpBin: "php8.3", runDir: "/public/", port: "8080", serverName: " a.com   b.com ", rewrite: "frontController" as const };
    const config = phpConfigFromDraft(draft);
    expect(config).toMatchObject({ phpBin: "php8.3", runDir: "public", port: 8080, serverName: "a.com b.com", rewrite: "frontController" });
    const restored = draftFromPhpApp(phpApp({ php: config, setupStatus: "ready" }));
    expect(restored.setupStep).toBe(4);
    expect(restored.processManager).toBe("dynamic");
    expect(phpConfigFromDraft(restored)).toEqual(config);
  });
});

describe("PhpProjectPanel", () => {
  it("renders the empty state with a single add action", async () => {
    await act(async () => {
      root.render(<PhpProjectPanel sessionId="session-1" profileId="profile-1" apps={[]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain(i18n.t("deployment.php.emptyTitle"));
    expect(container.querySelectorAll("button").length).toBe(1);
  });

  it("disables deploy while disconnected", async () => {
    await act(async () => {
      root.render(<PhpProjectPanel sessionId={null} profileId="profile-1" apps={[phpApp()]} onChanged={async () => undefined} />);
    });
    const deploy = Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes(i18n.t("deployment.python.deploy")));
    expect(deploy?.hasAttribute("disabled")).toBe(true);
    expect(invoke).not.toHaveBeenCalledWith("deployment_python_status", expect.anything());
  });

  it("shows the live php-fpm state and asks for confirmation before controlling the service", async () => {
    await act(async () => {
      root.render(<PhpProjectPanel sessionId="session-1" profileId="profile-1" apps={[phpApp()]} onChanged={async () => undefined} />);
    });
    expect(invoke).toHaveBeenCalledWith("deployment_python_status", { request: { sessionId: "session-1", service: "runory-php-shop.service" } });
    expect(container.textContent).toContain(i18n.t("deployment.python.status.active"));

    const restart = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === i18n.t("deployment.python.action.restart"));
    await act(async () => { restart?.click(); });
    expect(invoke).not.toHaveBeenCalledWith("deployment_php_control", expect.anything());
    expect(document.body.textContent).toContain(i18n.t("deployment.php.controlConfirm", { name: "shop", action: i18n.t("deployment.python.action.restart") }));

    const confirm = Array.from(document.querySelectorAll("button")).find((button) => button.textContent === i18n.t("deployment.confirm"));
    await act(async () => { confirm?.click(); });
    expect(invoke).toHaveBeenCalledWith("deployment_php_control", { request: { sessionId: "session-1", appId: "app-1", action: "restart" } });
  });

  it("does not offer service controls for unfinished drafts", async () => {
    await act(async () => {
      root.render(<PhpProjectPanel sessionId="session-1" profileId="profile-1" apps={[phpApp({ setupStatus: "draft", setupStep: 2, php: null, restart: { kind: "none" } })]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain(i18n.t("deployment.python.continue"));
    expect(container.textContent).not.toContain(i18n.t("deployment.python.logs"));
  });
});
