import type {
  DeploymentApp,
  DeploymentSetupStatus,
  DeploymentSourceKind,
  PhpProcessManager,
  PhpProjectConfig,
  PhpRewriteMode,
} from "../../types/infrastructure";

export type PhpProjectDraft = {
  id?: string;
  name: string;
  projectPath: string;
  setupStep: 1 | 2 | 3 | 4;
  setupStatus: DeploymentSetupStatus;
  sourceKind: DeploymentSourceKind;
  remoteUrl: string;
  branch: string;
  phpBin: string;
  runDir: string;
  composerInstall: boolean;
  port: string;
  serverName: string;
  rewrite: PhpRewriteMode;
  runUser: string;
  processManager: PhpProcessManager;
  maxChildren: string;
  memoryLimitMb: string;
  maxExecutionSeconds: string;
  uploadMaxMb: string;
  openBasedir: boolean;
  autoStart: boolean;
};

export function emptyPhpDraft(): PhpProjectDraft {
  return {
    name: "",
    projectPath: "",
    setupStep: 1,
    setupStatus: "draft",
    sourceKind: "existingPath",
    remoteUrl: "",
    branch: "master",
    phpBin: "php",
    runDir: "",
    composerInstall: true,
    port: "80",
    serverName: "",
    rewrite: "none",
    runUser: "www",
    processManager: "dynamic",
    maxChildren: "8",
    memoryLimitMb: "256",
    maxExecutionSeconds: "60",
    uploadMaxMb: "50",
    openBasedir: true,
    autoStart: true,
  };
}

export function draftFromPhpApp(app: DeploymentApp): PhpProjectDraft {
  const php = app.php;
  const base = emptyPhpDraft();
  const finished = app.setupStatus === "ready" || app.setupStatus === "provisioned" || app.setupStatus === "failed";
  const step = finished ? 4 : (Math.min(Math.max(app.setupStep ?? 1, 1), 4) as 1 | 2 | 3 | 4);
  return {
    ...base,
    id: app.id,
    name: app.name,
    projectPath: app.repositoryPath,
    setupStep: step,
    setupStatus: app.setupStatus ?? (php ? "ready" : "draft"),
    sourceKind: app.sourceKind ?? (app.remoteUrl ? "gitClone" : "existingPath"),
    remoteUrl: app.remoteUrl ?? "",
    branch: app.branch || "master",
    phpBin: php?.phpBin ?? base.phpBin,
    runDir: php?.runDir ?? base.runDir,
    composerInstall: php?.composerInstall ?? base.composerInstall,
    port: String(php?.port ?? base.port),
    serverName: php?.serverName ?? base.serverName,
    rewrite: php?.rewrite ?? base.rewrite,
    runUser: php?.runUser ?? base.runUser,
    processManager: php?.processManager ?? base.processManager,
    maxChildren: String(php?.maxChildren ?? base.maxChildren),
    memoryLimitMb: String(php?.memoryLimitMb ?? base.memoryLimitMb),
    maxExecutionSeconds: String(php?.maxExecutionSeconds ?? base.maxExecutionSeconds),
    uploadMaxMb: String(php?.uploadMaxMb ?? base.uploadMaxMb),
    openBasedir: php?.openBasedir ?? base.openBasedir,
    autoStart: php?.autoStart ?? base.autoStart,
  };
}

export function normalizePhpDraft(draft: PhpProjectDraft): PhpProjectDraft {
  return {
    ...draft,
    name: draft.name.trim(),
    projectPath: draft.projectPath.trim(),
    remoteUrl: draft.remoteUrl.trim(),
    branch: draft.branch.trim() || "master",
    phpBin: draft.phpBin.trim(),
    runDir: draft.runDir.trim().replace(/^\/+|\/+$/g, ""),
    port: draft.port.trim(),
    serverName: draft.serverName.trim().split(/\s+/).filter(Boolean).join(" "),
    runUser: draft.runUser.trim(),
    maxChildren: draft.maxChildren.trim(),
    memoryLimitMb: draft.memoryLimitMb.trim(),
    maxExecutionSeconds: draft.maxExecutionSeconds.trim(),
    uploadMaxMb: draft.uploadMaxMb.trim(),
  };
}

// PHP paths end up in nginx and php-fpm configuration, so the character set mirrors the Rust validator.
const safePath = /^[\w./+@-]+$/;
const hostName = /^(?=.{1,253}$)([A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)+[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?$/;

function inRange(value: string, min: number, max: number): boolean {
  return /^\d+$/.test(value) && Number(value) >= min && Number(value) <= max;
}

export function validatePhpStep(draft: PhpProjectDraft, step: 1 | 2 | 3 | 4): string | null {
  const next = normalizePhpDraft(draft);
  if (step === 1) {
    if (!next.name || next.name.length > 128) return "deployment.validation.name";
    if (!next.projectPath.startsWith("/") || next.projectPath.split("/").includes("..")) {
      return "deployment.validation.repositoryPath";
    }
    if (!safePath.test(next.projectPath)) return "deployment.php.validation.pathChars";
    return null;
  }
  if (step === 2) {
    if (next.sourceKind !== "gitClone") return null;
    if (!next.remoteUrl) return "deployment.validation.remoteUrl";
    if (!next.branch) return "deployment.validation.branch";
    return null;
  }
  if (step === 3) {
    if (!/^php(\d[\d.]{0,5})?$/.test(next.phpBin)) return "deployment.php.validation.phpBin";
    if (next.runDir && (next.runDir.split("/").includes("..") || !safePath.test(next.runDir))) {
      return "deployment.php.validation.runDir";
    }
    return null;
  }
  return validatePhpDraft({ ...next, setupStep: 4 });
}

export function validatePhpDraft(draft: PhpProjectDraft): string | null {
  const next = normalizePhpDraft(draft);
  for (const step of [1, 2, 3] as const) {
    const error = validatePhpStep(next, step);
    if (error) return error;
  }
  if (!inRange(next.port, 1, 65535)) return "deployment.php.validation.port";
  const names = next.serverName ? next.serverName.split(" ") : [];
  if (names.length > 10 || names.some((name) => name !== "localhost" && !hostName.test(name))) {
    return "deployment.php.validation.serverName";
  }
  if (!next.runUser || !/^[\w.@:-]+$/.test(next.runUser)) return "deployment.php.validation.runUser";
  if (
    !inRange(next.maxChildren, 1, 512)
    || !inRange(next.memoryLimitMb, 16, 8192)
    || !inRange(next.maxExecutionSeconds, 1, 3600)
    || !inRange(next.uploadMaxMb, 1, 2048)
  ) {
    return "deployment.php.validation.limits";
  }
  return null;
}

export function phpConfigFromDraft(draft: PhpProjectDraft): PhpProjectConfig {
  const next = normalizePhpDraft(draft);
  return {
    phpBin: next.phpBin || "php",
    port: Number(next.port) || 0,
    serverName: next.serverName,
    runDir: next.runDir,
    rewrite: next.rewrite,
    runUser: next.runUser,
    processManager: next.processManager,
    maxChildren: Number(next.maxChildren),
    memoryLimitMb: Number(next.memoryLimitMb),
    maxExecutionSeconds: Number(next.maxExecutionSeconds),
    uploadMaxMb: Number(next.uploadMaxMb),
    openBasedir: next.openBasedir,
    composerInstall: next.composerInstall,
    autoStart: next.autoStart,
  };
}
