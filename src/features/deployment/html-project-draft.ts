import type {
  DeploymentApp,
  DeploymentSetupStatus,
  DeploymentSourceKind,
  HtmlProjectConfig,
} from "../../types/infrastructure";

export type HtmlProjectDraft = {
  id?: string;
  name: string;
  projectPath: string;
  setupStep: 1 | 2 | 3 | 4;
  setupStatus: DeploymentSetupStatus;
  sourceKind: DeploymentSourceKind;
  remoteUrl: string;
  branch: string;
  rootDir: string;
  indexFiles: string;
  spaFallback: boolean;
  port: string;
  serverName: string;
  gzip: boolean;
  cacheStatic: boolean;
};

export function emptyHtmlDraft(): HtmlProjectDraft {
  return {
    name: "",
    projectPath: "",
    setupStep: 1,
    setupStatus: "draft",
    sourceKind: "existingPath",
    remoteUrl: "",
    branch: "master",
    rootDir: "",
    indexFiles: "index.html index.htm",
    spaFallback: false,
    port: "80",
    serverName: "",
    gzip: true,
    cacheStatic: true,
  };
}

export function draftFromHtmlApp(app: DeploymentApp): HtmlProjectDraft {
  const html = app.html;
  const base = emptyHtmlDraft();
  const finished = app.setupStatus === "ready" || app.setupStatus === "provisioned" || app.setupStatus === "failed";
  const step = finished ? 4 : (Math.min(Math.max(app.setupStep ?? 1, 1), 4) as 1 | 2 | 3 | 4);
  return {
    ...base,
    id: app.id,
    name: app.name,
    projectPath: app.repositoryPath,
    setupStep: step,
    setupStatus: app.setupStatus ?? (html ? "ready" : "draft"),
    sourceKind: app.sourceKind ?? (app.remoteUrl ? "gitClone" : "existingPath"),
    remoteUrl: app.remoteUrl ?? "",
    branch: app.branch || "master",
    rootDir: html?.rootDir ?? base.rootDir,
    indexFiles: html?.indexFiles ?? base.indexFiles,
    spaFallback: html?.spaFallback ?? base.spaFallback,
    port: String(html?.port ?? base.port),
    serverName: html?.serverName ?? base.serverName,
    gzip: html?.gzip ?? base.gzip,
    cacheStatic: html?.cacheStatic ?? base.cacheStatic,
  };
}

const collapse = (value: string) => value.trim().split(/\s+/).filter(Boolean).join(" ");

export function normalizeHtmlDraft(draft: HtmlProjectDraft): HtmlProjectDraft {
  return {
    ...draft,
    name: draft.name.trim(),
    projectPath: draft.projectPath.trim(),
    remoteUrl: draft.remoteUrl.trim(),
    branch: draft.branch.trim() || "master",
    rootDir: draft.rootDir.trim().replace(/^\/+|\/+$/g, ""),
    indexFiles: collapse(draft.indexFiles),
    port: draft.port.trim(),
    serverName: collapse(draft.serverName),
  };
}

// Paths end up in nginx configuration, so the character set mirrors the Rust validator.
const safePath = /^[\w./+@-]+$/;
const hostName = /^(?=.{1,253}$)([A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)+[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?$/;
const indexFile = /^[A-Za-z0-9_][\w.-]{0,63}$/;

export function validateHtmlStep(draft: HtmlProjectDraft, step: 1 | 2 | 3 | 4): string | null {
  const next = normalizeHtmlDraft(draft);
  if (step === 1) {
    if (!next.name || next.name.length > 128) return "deployment.validation.name";
    if (!next.projectPath.startsWith("/") || next.projectPath.split("/").includes("..")) {
      return "deployment.validation.repositoryPath";
    }
    if (!safePath.test(next.projectPath)) return "deployment.html.validation.pathChars";
    return null;
  }
  if (step === 2) {
    if (next.sourceKind !== "gitClone") return null;
    if (!next.remoteUrl) return "deployment.validation.remoteUrl";
    if (!next.branch) return "deployment.validation.branch";
    return null;
  }
  if (step === 3) {
    if (next.rootDir && (next.rootDir.length > 256 || next.rootDir.split("/").includes("..") || !safePath.test(next.rootDir))) {
      return "deployment.html.validation.rootDir";
    }
    const files = next.indexFiles ? next.indexFiles.split(" ") : [];
    if (files.length < 1 || files.length > 8 || files.some((file) => !indexFile.test(file))) {
      return "deployment.html.validation.indexFiles";
    }
    return null;
  }
  return validateHtmlDraft({ ...next, setupStep: 4 });
}

export function validateHtmlDraft(draft: HtmlProjectDraft): string | null {
  const next = normalizeHtmlDraft(draft);
  for (const step of [1, 2, 3] as const) {
    const error = validateHtmlStep(next, step);
    if (error) return error;
  }
  if (!/^\d+$/.test(next.port) || Number(next.port) < 1 || Number(next.port) > 65535) return "deployment.html.validation.port";
  const names = next.serverName ? next.serverName.split(" ") : [];
  if (names.length > 10 || names.some((name) => name !== "localhost" && !hostName.test(name.replace(/^\*\./, "")))) {
    return "deployment.html.validation.serverName";
  }
  return null;
}

export function htmlConfigFromDraft(draft: HtmlProjectDraft): HtmlProjectConfig {
  const next = normalizeHtmlDraft(draft);
  return {
    port: Number(next.port) || 0,
    serverName: next.serverName,
    rootDir: next.rootDir,
    indexFiles: next.indexFiles,
    spaFallback: next.spaFallback,
    gzip: next.gzip,
    cacheStatic: next.cacheStatic,
  };
}
