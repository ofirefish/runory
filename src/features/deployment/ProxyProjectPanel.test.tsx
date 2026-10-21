// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import type { DeploymentApp } from "../../types/infrastructure";
import { draftFromProxyApp, emptyProxyDraft, proxyConfigFromDraft, validateProxyDraft, validateProxyStep } from "./proxy-project-draft";
import { ProxyProjectPanel } from "./ProxyProjectPanel";

const invoke = vi.hoisted(() => vi.fn(async (command: string) => {
  if (command === "plugin_workspace_entries") return [];
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

const readyDraft = () => ({ ...emptyProxyDraft(), name: "api", targetUrl: "http://127.0.0.1:3000", setupStep: 3 as const });

function proxyApp(overrides: Partial<DeploymentApp> = {}): DeploymentApp {
  return {
    id: "app-1",
    profileId: "profile-1",
    name: "api",
    repositoryPath: "/etc/nginx/conf.d/runory-proxy-api.conf",
    remoteUrl: "",
    branch: "main",
    build: "none",
    restart: { kind: "none" },
    projectKind: "reverseProxy",
    proxy: proxyConfigFromDraft(readyDraft()),
    setupStep: 3,
    setupStatus: "provisioned",
    sourceKind: "existingPath",
    venvPath: "",
    updatedAtEpochSeconds: 1,
    ...overrides,
  };
}

describe("reverse proxy draft validation", () => {
  it("accepts the default final-step draft", () => {
    expect(validateProxyDraft(readyDraft())).toBeNull();
  });

  it("accepts plain http(s) targets and rejects anything that could alter the directive", () => {
    for (const ok of ["http://127.0.0.1:3000", "https://api.example.com", "http://backend:8080/v1/", "http://localhost"]) {
      expect(validateProxyStep({ ...readyDraft(), targetUrl: ok }, 1), ok).toBeNull();
    }
    for (const bad of ["", "ftp://x.com", "127.0.0.1:3000", "http://", "http://user@host", "http://host:99999", "http://host/a b", "http://host/a;b", "http://host/$x", "http://host/a?x=1", "http://host/../etc", "http://-host"]) {
      expect(validateProxyStep({ ...readyDraft(), targetUrl: bad }, 1), bad).toBe("deployment.proxy.validation.targetUrl");
    }
    expect(validateProxyStep({ ...readyDraft(), name: "" }, 1)).toBe("deployment.validation.name");
  });

  it("validates port, domains and limits", () => {
    expect(validateProxyStep({ ...readyDraft(), port: "0" }, 2)).toBe("deployment.proxy.validation.port");
    expect(validateProxyStep({ ...readyDraft(), serverName: "a.com *.a.com localhost" }, 2)).toBeNull();
    expect(validateProxyStep({ ...readyDraft(), serverName: "a.com; root /;" }, 2)).toBe("deployment.proxy.validation.serverName");
    expect(validateProxyDraft({ ...readyDraft(), maxBodyMb: "0" })).toBe("deployment.proxy.validation.limits");
    expect(validateProxyDraft({ ...readyDraft(), timeoutSeconds: "4000" })).toBe("deployment.proxy.validation.limits");
  });

  it("round-trips a saved proxy through the draft", () => {
    const draft = { ...readyDraft(), port: "8080", serverName: " a.com   b.com ", hostHeader: "target" as const, websocket: false };
    const config = proxyConfigFromDraft(draft);
    expect(config).toMatchObject({ port: 8080, serverName: "a.com b.com", hostHeader: "target", websocket: false, maxBodyMb: 50, timeoutSeconds: 60 });
    const restored = draftFromProxyApp(proxyApp({ proxy: config, setupStatus: "ready" }));
    expect(restored.setupStep).toBe(3);
    expect(proxyConfigFromDraft(restored)).toEqual(config);
  });
});

describe("ProxyProjectPanel", () => {
  it("renders the empty state with a single add action", async () => {
    await act(async () => {
      root.render(<ProxyProjectPanel sessionId="session-1" profileId="profile-1" apps={[]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain(i18n.t("deployment.proxy.emptyTitle"));
    expect(container.querySelectorAll("button").length).toBe(1);
  });

  it("disables deploy while disconnected and shows the target", async () => {
    await act(async () => {
      root.render(<ProxyProjectPanel sessionId={null} profileId="profile-1" apps={[proxyApp()]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain("http://127.0.0.1:3000");
    const deploy = Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes(i18n.t("deployment.python.deploy")));
    expect(deploy?.hasAttribute("disabled")).toBe(true);
  });

  it("asks for confirmation before deploying and records the result", async () => {
    await act(async () => {
      root.render(<ProxyProjectPanel sessionId="session-1" profileId="profile-1" apps={[proxyApp()]} onChanged={async () => undefined} />);
    });
    const deploy = Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes(i18n.t("deployment.python.deploy")));
    await act(async () => { deploy?.click(); });
    expect(invoke).not.toHaveBeenCalledWith("deployment_proxy_provision", expect.anything());
    expect(document.body.textContent).toContain(i18n.t("deployment.proxy.provisionConfirm", { name: "api" }));

    const confirm = Array.from(document.querySelectorAll("button")).find((button) => button.textContent === i18n.t("deployment.confirm"));
    await act(async () => { confirm?.click(); });
    expect(invoke).toHaveBeenCalledWith("deployment_proxy_provision", {
      request: { sessionId: "session-1", name: "api", proxy: proxyConfigFromDraft(readyDraft()) },
    });
    const statuses = invoke.mock.calls
      .filter((call) => call[0] === "deployment_apps_upsert")
      .map((call) => (call as unknown as [string, { request: { setupStatus: string; repositoryPath: string } }])[1].request);
    expect(statuses.map((request) => request.setupStatus)).toEqual(["provisioning", "provisioned"]);
  });

  it("does not offer log access for unfinished drafts", async () => {
    await act(async () => {
      root.render(<ProxyProjectPanel sessionId="session-1" profileId="profile-1" apps={[proxyApp({ setupStatus: "draft", setupStep: 2, proxy: null })]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain(i18n.t("deployment.python.continue"));
    expect(container.textContent).not.toContain(i18n.t("deployment.python.logs"));
  });
});
