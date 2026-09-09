import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import type { BuildPreset, DeploymentApp, RestartTarget } from "../../types/infrastructure";
import { DeploymentInputField as Field, DeploymentSelectField as SelectField } from "./DeploymentFields";

export type DeploymentAppDraft = {
  id?: string;
  name: string;
  repositoryPath: string;
  remoteUrl: string;
  branch: string;
  build: BuildPreset;
  restartKind: "none" | "systemd" | "pm2" | "dockerCompose";
  restartName: string;
};

export function emptyAppDraft(): DeploymentAppDraft {
  return {
    name: "",
    repositoryPath: "",
    remoteUrl: "",
    branch: "",
    build: "none",
    restartKind: "none",
    restartName: "",
  };
}

export function draftFromApp(app: DeploymentApp): DeploymentAppDraft {
  const restartKind = app.restart.kind;
  const restartName =
    app.restart.kind === "systemd" || app.restart.kind === "dockerCompose"
      ? app.restart.service
      : app.restart.kind === "pm2"
        ? app.restart.process
        : "";
  return {
    id: app.id,
    name: app.name,
    repositoryPath: app.repositoryPath,
    remoteUrl: app.remoteUrl,
    branch: app.branch,
    build: app.build,
    restartKind,
    restartName,
  };
}

export function restartFromDraft(draft: DeploymentAppDraft): RestartTarget {
  if (draft.restartKind === "none") return { kind: "none" };
  if (draft.restartKind === "systemd") return { kind: "systemd", service: draft.restartName };
  if (draft.restartKind === "pm2") return { kind: "pm2", process: draft.restartName };
  return { kind: "dockerCompose", service: draft.restartName };
}

export function DeploymentAppForm({
  title,
  initial,
  onClose,
  onSubmit,
}: {
  title: string;
  initial: DeploymentAppDraft;
  onClose: () => void;
  onSubmit: (draft: DeploymentAppDraft) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const set = <K extends keyof DeploymentAppDraft>(key: K, value: DeploymentAppDraft[K]) =>
    setDraft((current) => ({ ...current, [key]: value }));

  const save = async () => {
    setBusy(true);
    setFailed(false);
    try {
      await onSubmit(draft);
      onClose();
    } catch {
      setFailed(true);
      setBusy(false);
    }
  };

  return (
    <DialogShell title={title} onClose={onClose} size="form">
      <div className="grid gap-4">
        <Field label={t("deployment.appName")} value={draft.name} onChange={(value) => set("name", value)} placeholder={t("deployment.placeholder.appName")} />
        <Field label={t("deployment.repositoryPath")} value={draft.repositoryPath} onChange={(value) => set("repositoryPath", value)} placeholder={t("deployment.placeholder.repositoryPath")} />
        <Field label={t("deployment.remoteUrl")} value={draft.remoteUrl} onChange={(value) => set("remoteUrl", value)} placeholder={t("deployment.placeholder.remoteUrl")} />
        <Field label={t("deployment.branch")} value={draft.branch} onChange={(value) => set("branch", value)} placeholder={t("deployment.placeholder.branch")} />
        <SelectField
          label={t("deployment.buildPreset")}
          value={draft.build}
          onChange={(value) => set("build", value)}
          options={(["none", "npm", "pnpm", "cargo"] as const).map((value) => ({ value, label: t(`deployment.build.${value}`) }))}
        />
        <SelectField
          label={t("deployment.restartTarget")}
          value={draft.restartKind}
          onChange={(value) => set("restartKind", value)}
          options={(["none", "systemd", "pm2", "dockerCompose"] as const).map((value) => ({ value, label: t(`deployment.restart.${value}`) }))}
        />
        {draft.restartKind !== "none" && (
          <Field label={t("deployment.restartName")} value={draft.restartName} onChange={(value) => set("restartName", value)} placeholder={t("deployment.placeholder.restartName")} />
        )}
        {failed && <p className="text-sm text-red-500">{t("deployment.error")}</p>}
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>{t("common.cancel")}</Button>
          <Button disabled={busy || !draft.name.trim()} onClick={() => void save()}>{t("common.save")}</Button>
        </div>
      </div>
    </DialogShell>
  );
}
