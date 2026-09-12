import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { useSettingsStore } from "../../stores/settings-store";

type CliKind = "boundary" | "teleport";

export function HelperCliSettings() {
  const { t } = useTranslation();
  const {
    boundaryCliPath,
    teleportCliPath,
    saving,
    persistenceError,
    setBoundaryCliPath,
    setTeleportCliPath,
  } = useSettingsStore();
  const [boundaryDraft, setBoundaryDraft] = useState(boundaryCliPath);
  const [teleportDraft, setTeleportDraft] = useState(teleportCliPath);

  useEffect(() => {
    setBoundaryDraft(boundaryCliPath);
  }, [boundaryCliPath]);
  useEffect(() => {
    setTeleportDraft(teleportCliPath);
  }, [teleportCliPath]);

  const browse = async (kind: CliKind) => {
    const selected = await open({
      multiple: false,
      directory: false,
      title: t(kind === "boundary" ? "settings.helpers.browseBoundary" : "settings.helpers.browseTeleport"),
    });
    if (typeof selected !== "string" || !selected.trim()) return;
    if (kind === "boundary") {
      setBoundaryDraft(selected);
      await setBoundaryCliPath(selected);
    } else {
      setTeleportDraft(selected);
      await setTeleportCliPath(selected);
    }
  };

  return (
    <div className="settings-stack">
      <section className="settings-card">
        <h4>{t("settings.helpers.boundaryCliPath")}</h4>
        <p>{t("settings.helpers.boundaryHint")}</p>
        <div className="mt-3 flex gap-2">
          <Input
            id="settings-boundary-cli"
            className="min-w-0 flex-1 font-mono text-xs"
            autoCapitalize="none"
            spellCheck={false}
            value={boundaryDraft}
            placeholder={t("bastion.boundaryCliPlaceholder")}
            aria-label={t("settings.helpers.boundaryCliPath")}
            onChange={(event) => setBoundaryDraft(event.target.value)}
            onBlur={() => {
              if (boundaryDraft.trim() !== boundaryCliPath) void setBoundaryCliPath(boundaryDraft);
            }}
          />
          <Button
            type="button"
            variant="secondary"
            className="shrink-0"
            onClick={() => void browse("boundary")}
          >
            <FolderOpen size={16} aria-hidden="true" />
            <span className="hidden sm:inline">{t("settings.helpers.browse")}</span>
          </Button>
          {boundaryCliPath ? (
            <Button
              type="button"
              variant="ghost"
              className="shrink-0"
              aria-label={t("settings.helpers.clear")}
              onClick={() => {
                setBoundaryDraft("");
                void setBoundaryCliPath("");
              }}
            >
              <X size={16} aria-hidden="true" />
            </Button>
          ) : null}
        </div>
      </section>

      <section className="settings-card">
        <h4>{t("settings.helpers.teleportCliPath")}</h4>
        <p>{t("settings.helpers.teleportHint")}</p>
        <div className="mt-3 flex gap-2">
          <Input
            id="settings-teleport-cli"
            className="min-w-0 flex-1 font-mono text-xs"
            autoCapitalize="none"
            spellCheck={false}
            value={teleportDraft}
            placeholder={t("bastion.teleportCliPlaceholder")}
            aria-label={t("settings.helpers.teleportCliPath")}
            onChange={(event) => setTeleportDraft(event.target.value)}
            onBlur={() => {
              if (teleportDraft.trim() !== teleportCliPath) void setTeleportCliPath(teleportDraft);
            }}
          />
          <Button
            type="button"
            variant="secondary"
            className="shrink-0"
            onClick={() => void browse("teleport")}
          >
            <FolderOpen size={16} aria-hidden="true" />
            <span className="hidden sm:inline">{t("settings.helpers.browse")}</span>
          </Button>
          {teleportCliPath ? (
            <Button
              type="button"
              variant="ghost"
              className="shrink-0"
              aria-label={t("settings.helpers.clear")}
              onClick={() => {
                setTeleportDraft("");
                void setTeleportCliPath("");
              }}
            >
              <X size={16} aria-hidden="true" />
            </Button>
          ) : null}
        </div>
      </section>

      {saving && (
        <p role="status" className="text-xs text-[hsl(var(--muted))]">
          {t("settings.saving")}
        </p>
      )}
      {persistenceError && (
        <p role="alert" className="text-xs text-red-500">
          {t("settings.persistenceError")}
        </p>
      )}
    </div>
  );
}
