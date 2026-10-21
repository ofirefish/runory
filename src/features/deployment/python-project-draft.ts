import type { DeploymentApp, DeploymentSetupStatus, DeploymentSourceKind, PythonProjectConfig, PythonProtocol, PythonStartupMethod } from "../../types/infrastructure";

export type PythonProjectDraft = {
  id?: string;
  name: string;
  port: string;
  pythonBin: string;
  startup: PythonStartupMethod;
  projectPath: string;
  entryFile: string;
  protocol: PythonProtocol;
  application: string;
  startupCommand: string;
  runUser: string;
  requirementsPath: string;
  setupStep: 1 | 2 | 3 | 4;
  setupStatus: DeploymentSetupStatus;
  sourceKind: DeploymentSourceKind;
  remoteUrl: string;
  branch: string;
  venvPath: string;
  useExistingVenv: boolean;
  processes: string;
  threads: string;
  autoStart: boolean;
};

export function emptyPythonDraft(): PythonProjectDraft {
  return {
    name: "",
    port: "8000",
    pythonBin: "python3",
    startup: "gunicorn",
    projectPath: "",
    entryFile: "main.py",
    protocol: "asgi",
    application: "",
    startupCommand: "",
    runUser: "www",
    requirementsPath: "requirements.txt",
    setupStep: 1,
    setupStatus: "draft",
    sourceKind: "existingPath",
    remoteUrl: "",
    branch: "master",
    venvPath: "",
    useExistingVenv: false,
    processes: "1",
    threads: "1",
    autoStart: true,
  };
}

export function draftFromPythonApp(app: DeploymentApp): PythonProjectDraft {
  const python = app.python;
  const step = mapPersistedSetupStep(app);
  return {
    id: app.id,
    name: app.name,
    port: String(python?.port ?? 8000),
    pythonBin: python?.pythonBin ?? "python3",
    startup: python?.startup ?? "gunicorn",
    projectPath: app.repositoryPath,
    entryFile: python?.entryFile ?? "main.py",
    protocol: python?.protocol ?? "asgi",
    application: python?.application ?? "",
    startupCommand: python?.startupCommand ?? "",
    runUser: python?.runUser ?? "www",
    requirementsPath: python?.requirementsPath ?? "requirements.txt",
    setupStep: step,
    setupStatus: app.setupStatus ?? (python ? "ready" : "draft"),
    sourceKind: app.sourceKind ?? (app.remoteUrl ? "gitClone" : "existingPath"),
    remoteUrl: app.remoteUrl ?? "",
    branch: app.branch || "master",
    venvPath: app.venvPath ?? "",
    useExistingVenv: Boolean(app.venvPath),
    processes: String(python?.processes ?? 1),
    threads: String(python?.threads ?? 1),
    autoStart: python?.autoStart ?? true,
  };
}

/** Maps persisted wizard steps onto the 4-step flow (Git inserted as step 2). */
function mapPersistedSetupStep(app: DeploymentApp): 1 | 2 | 3 | 4 {
  const status = app.setupStatus;
  if (status === "ready" || status === "provisioned" || status === "failed") return 4;
  const step = app.setupStep ?? 1;
  if (step === 1) return 1;
  if (step === 4) return 4;
  // Pre-Git-step drafts: step 2 was environment, step 3 was startup.
  if (step === 3) return 4;
  if (step === 2 && app.python) return 3;
  if (step === 2) return 2;
  return 1;
}

export function normalizePythonDraft(draft: PythonProjectDraft): PythonProjectDraft {
  return {
    ...draft,
    name: draft.name.trim(),
    port: draft.port.trim(),
    pythonBin: draft.pythonBin.trim(),
    projectPath: draft.projectPath.trim(),
    entryFile: draft.entryFile.trim(),
    application: draft.application.trim(),
    startupCommand: draft.startupCommand.trim(),
    runUser: draft.runUser.trim(),
    requirementsPath: draft.requirementsPath.trim(),
    remoteUrl: draft.remoteUrl.trim(),
    branch: draft.branch.trim() || "master",
    venvPath: draft.venvPath.trim(),
  };
}

function isSafeCommandArg(part: string): boolean {
  return part.length > 0 && part.length <= 256 && !part.includes("..") && /^[\w./=:+@-]+$/.test(part);
}

export function validatePythonStep(draft: PythonProjectDraft, step: 1 | 2 | 3 | 4): string | null {
  const next = normalizePythonDraft(draft);
  if (step === 1) {
    if (!next.name || next.name.length > 128) return "deployment.validation.name";
    if (!next.projectPath.startsWith("/") || next.projectPath.length > 4096 || next.projectPath.split("/").includes("..")) {
      return "deployment.validation.repositoryPath";
    }
    return null;
  }
  if (step === 2) {
    if (next.sourceKind !== "gitClone") return null;
    if (!next.remoteUrl) return "deployment.validation.remoteUrl";
    if (!next.branch) return "deployment.validation.branch";
    return null;
  }
  if (step === 3) {
    if (!next.pythonBin) return "deployment.python.validation.pythonBin";
    if (next.useExistingVenv && (!next.venvPath.startsWith("/") || next.venvPath.split("/").includes(".."))) {
      return "deployment.python.validation.venvPath";
    }
    if (next.requirementsPath && next.requirementsPath.split("/").includes("..")) {
      return "deployment.python.validation.requirementsPath";
    }
    return null;
  }
  return validatePythonDraft({ ...next, setupStep: 4 });
}

export function validatePythonDraft(draft: PythonProjectDraft): string | null {
  const next = normalizePythonDraft(draft);
  const step1 = validatePythonStep(next, 1);
  if (step1) return step1;
  if (next.sourceKind === "gitClone") {
    const step2 = validatePythonStep({ ...next, sourceKind: "gitClone" }, 2);
    if (step2) return step2;
  }
  const step3 = validatePythonStep(next, 3);
  if (step3) return step3;
  if (!next.runUser || !/^[\w.@:-]+$/.test(next.runUser)) return "deployment.python.validation.runUser";
  for (const count of [next.processes, next.threads]) {
    if (!/^\d+$/.test(count) || Number(count) < 1 || Number(count) > 64) return "deployment.python.validation.concurrency";
  }
  if (next.startup === "command") {
    if (!next.startupCommand || next.startupCommand.length > 512) return "deployment.python.validation.startupCommand";
    if (/[;|&$`<>(){}[\]\\"'\n\r]/.test(next.startupCommand) || next.startupCommand.includes("..")) {
      return "deployment.python.validation.startupCommand";
    }
    const parts = next.startupCommand.split(/\s+/).filter(Boolean);
    const first = parts[0];
    if (!first || first.startsWith("/") || first.split("/").includes("..")) return "deployment.python.validation.startupCommand";
    if (!parts.every(isSafeCommandArg)) return "deployment.python.validation.startupCommand";
    return null;
  }
  const port = Number(next.port);
  if (!Number.isInteger(port) || port < 1 || port > 65535) return "deployment.python.validation.port";
  if (next.startup === "uwsgi") {
    if (!next.entryFile || next.entryFile.startsWith("/") || next.entryFile.split("/").includes("..")) {
      return "deployment.python.validation.entryFile";
    }
    if (next.protocol === "asgi") return "deployment.python.validation.uwsgiAsgi";
    if (next.application && !/^[A-Za-z_][A-Za-z0-9_]*$/.test(next.application)) return "deployment.python.validation.callable";
    return null;
  }
  if (!next.application || next.application.length > 256 || /[\s\0\n\r]/.test(next.application)) {
    return "deployment.python.validation.application";
  }
  if (next.entryFile && (next.entryFile.startsWith("/") || next.entryFile.split("/").includes(".."))) {
    return "deployment.python.validation.entryFile";
  }
  return null;
}

export function pythonConfigFromDraft(draft: PythonProjectDraft): PythonProjectConfig {
  const next = normalizePythonDraft(draft);
  if (next.startup === "command") {
    return {
      port: 0,
      pythonBin: next.pythonBin || "python3",
      startup: "command",
      entryFile: "",
      protocol: "wsgi",
      application: "",
      runUser: next.runUser,
      requirementsPath: next.requirementsPath,
      startupCommand: next.startupCommand,
      processes: Number(next.processes),
      threads: Number(next.threads),
      autoStart: next.autoStart,
    };
  }
  return {
    port: Number(next.port) || 0,
    pythonBin: next.pythonBin || "python3",
    startup: next.startup,
    entryFile: next.entryFile,
    protocol: next.startup === "uwsgi" ? "wsgi" : next.protocol,
    application: next.application,
    runUser: next.runUser,
    requirementsPath: next.requirementsPath,
    startupCommand: "",
    processes: Number(next.processes),
    threads: Number(next.threads),
    autoStart: next.autoStart,
  };
}
