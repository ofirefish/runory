import type {
  DeploymentApp,
  DeploymentSetupStatus,
  ProxyHostHeader,
  ReverseProxyConfig,
} from "../../types/infrastructure";

export type ProxyProjectDraft = {
  id?: string;
  name: string;
  targetUrl: string;
  setupStep: 1 | 2 | 3;
  setupStatus: DeploymentSetupStatus;
  port: string;
  serverName: string;
  hostHeader: ProxyHostHeader;
  websocket: boolean;
  maxBodyMb: string;
  timeoutSeconds: string;
};

export function emptyProxyDraft(): ProxyProjectDraft {
  return {
    name: "",
    targetUrl: "",
    setupStep: 1,
    setupStatus: "draft",
    port: "80",
    serverName: "",
    hostHeader: "original",
    websocket: true,
    maxBodyMb: "50",
    timeoutSeconds: "60",
  };
}

export function draftFromProxyApp(app: DeploymentApp): ProxyProjectDraft {
  const proxy = app.proxy;
  const base = emptyProxyDraft();
  const finished = app.setupStatus === "ready" || app.setupStatus === "provisioned" || app.setupStatus === "failed";
  const step = finished ? 3 : (Math.min(Math.max(app.setupStep ?? 1, 1), 3) as 1 | 2 | 3);
  return {
    ...base,
    id: app.id,
    name: app.name,
    setupStep: step,
    setupStatus: app.setupStatus ?? (proxy ? "ready" : "draft"),
    targetUrl: proxy?.targetUrl ?? base.targetUrl,
    port: String(proxy?.port ?? base.port),
    serverName: proxy?.serverName ?? base.serverName,
    hostHeader: proxy?.hostHeader ?? base.hostHeader,
    websocket: proxy?.websocket ?? base.websocket,
    maxBodyMb: String(proxy?.maxBodyMb ?? base.maxBodyMb),
    timeoutSeconds: String(proxy?.timeoutSeconds ?? base.timeoutSeconds),
  };
}

export function normalizeProxyDraft(draft: ProxyProjectDraft): ProxyProjectDraft {
  return {
    ...draft,
    name: draft.name.trim(),
    targetUrl: draft.targetUrl.trim(),
    port: draft.port.trim(),
    serverName: draft.serverName.trim().split(/\s+/).filter(Boolean).join(" "),
    maxBodyMb: draft.maxBodyMb.trim(),
    timeoutSeconds: draft.timeoutSeconds.trim(),
  };
}

// The target is written into a proxy_pass directive, so the accepted shape mirrors the Rust validator.
const hostName = /^(?=.{1,253}$)([A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)+[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?$/;
const upstream = /^https?:\/\/([A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?)(?::(\d{1,5}))?((?:\/[\w.+@-]*)*)$/;

function validTarget(value: string): boolean {
  if (value.length > 512) return false;
  const match = upstream.exec(value);
  if (!match) return false;
  const [, host, port, path] = match;
  if (host.includes("..")) return false;
  if (port !== undefined && (Number(port) < 1 || Number(port) > 65535)) return false;
  return !(path ?? "").split("/").includes("..");
}

function inRange(value: string, min: number, max: number): boolean {
  return /^\d+$/.test(value) && Number(value) >= min && Number(value) <= max;
}

export function validateProxyStep(draft: ProxyProjectDraft, step: 1 | 2 | 3): string | null {
  const next = normalizeProxyDraft(draft);
  if (step === 1) {
    if (!next.name || next.name.length > 128) return "deployment.validation.name";
    if (!validTarget(next.targetUrl)) return "deployment.proxy.validation.targetUrl";
    return null;
  }
  if (step === 2) {
    if (!inRange(next.port, 1, 65535)) return "deployment.proxy.validation.port";
    const names = next.serverName ? next.serverName.split(" ") : [];
    if (names.length > 10 || names.some((name) => name !== "localhost" && !hostName.test(name.replace(/^\*\./, "")))) {
      return "deployment.proxy.validation.serverName";
    }
    return null;
  }
  return validateProxyDraft({ ...next, setupStep: 3 });
}

export function validateProxyDraft(draft: ProxyProjectDraft): string | null {
  const next = normalizeProxyDraft(draft);
  for (const step of [1, 2] as const) {
    const error = validateProxyStep(next, step);
    if (error) return error;
  }
  if (!inRange(next.maxBodyMb, 1, 2048) || !inRange(next.timeoutSeconds, 1, 3600)) {
    return "deployment.proxy.validation.limits";
  }
  return null;
}

export function proxyConfigFromDraft(draft: ProxyProjectDraft): ReverseProxyConfig {
  const next = normalizeProxyDraft(draft);
  return {
    port: Number(next.port) || 0,
    serverName: next.serverName,
    targetUrl: next.targetUrl,
    hostHeader: next.hostHeader,
    websocket: next.websocket,
    maxBodyMb: Number(next.maxBodyMb),
    timeoutSeconds: Number(next.timeoutSeconds),
  };
}
