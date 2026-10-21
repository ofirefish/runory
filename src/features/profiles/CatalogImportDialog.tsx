import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, FileUp, FolderOpen, Loader2 } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../components/ui/dialog";
import { applyCatalogImport, previewCatalogImport } from "../../lib/tauri/catalog";
import { useCatalogStore } from "../../stores/catalog-store";
import type { CatalogImportProfilePreview, CatalogImportSource } from "../../types/import";

const TERMIUS_EXPORT_URL = "https://github.com/y01and3/termius-export";

type SourceConfig = {
  source: CatalogImportSource;
  allowFile: boolean;
  allowDirectory: boolean;
  extensions: string[];
  showTermiusTip?: boolean;
};

const SOURCE_CONFIG: Record<CatalogImportSource, SourceConfig> = {
  terminus: { source: "terminus", allowFile: true, allowDirectory: false, extensions: ["yaml", "yml"], showTermiusTip: true },
  openssh: { source: "openssh", allowFile: true, allowDirectory: false, extensions: [] },
  xshell: { source: "xshell", allowFile: true, allowDirectory: true, extensions: ["xsh"] },
  securecrt: { source: "securecrt", allowFile: true, allowDirectory: true, extensions: ["ini"] },
  mobaxterm: { source: "mobaxterm", allowFile: true, allowDirectory: false, extensions: ["mxtsessions"] },
};

function appErrorCode(error: unknown): string {
  return typeof error === "object" && error !== null && "code" in error && typeof error.code === "string"
    ? error.code
    : "UNKNOWN";
}

function i18nPrefix(source: CatalogImportSource): string {
  return source === "terminus" ? "terminusImport" : `catalogImport.${source}`;
}

export function CatalogImportDialog({
  source,
  onClose,
}: {
  source: CatalogImportSource;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const load = useCatalogStore((store) => store.load);
  const config = SOURCE_CONFIG[source];
  const prefix = i18nPrefix(source);
  const [path, setPath] = useState<string | null>(null);
  const [profiles, setProfiles] = useState<CatalogImportProfilePreview[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [errorCode, setErrorCode] = useState<string | null>(null);

  const importable = useMemo(() => profiles.filter((profile) => profile.importable), [profiles]);
  const selectedCount = useMemo(
    () => importable.filter((profile) => selected.has(profile.importKey)).length,
    [importable, selected],
  );
  const selectedWithPassword = useMemo(
    () => importable.filter((profile) => selected.has(profile.importKey) && profile.hasPassword).length,
    [importable, selected],
  );

  const choosePath = async (directory: boolean) => {
    setErrorCode(null);
    const picked = await open({
      multiple: false,
      directory,
      title: t(`${prefix}.pickTitle`),
      filters: directory || config.extensions.length === 0
        ? undefined
        : [{ name: t(`${prefix}.fileFilter`), extensions: config.extensions }],
    });
    if (typeof picked !== "string" || !picked) return;
    setBusy(true);
    try {
      const preview = await previewCatalogImport(source, picked);
      setPath(picked);
      setProfiles(preview.profiles);
      setSelected(new Set(preview.profiles.filter((profile) => profile.importable).map((profile) => profile.importKey)));
    } catch (error) {
      setPath(null);
      setProfiles([]);
      setSelected(new Set());
      setErrorCode(appErrorCode(error));
    } finally {
      setBusy(false);
    }
  };

  const toggle = (key: string, enabled: boolean) => {
    setSelected((current) => {
      const next = new Set(current);
      if (enabled) next.add(key);
      else next.delete(key);
      return next;
    });
  };

  const toggleAll = (enabled: boolean) => {
    setSelected(enabled ? new Set(importable.map((profile) => profile.importKey)) : new Set());
  };

  const apply = async () => {
    if (!path || selectedCount === 0) return;
    setBusy(true);
    setErrorCode(null);
    try {
      const result = await applyCatalogImport(source, path, [...selected]);
      await load();
      toast.success(t("catalogImport.success", {
        created: result.profilesCreated,
        updated: result.profilesUpdated,
        groups: result.groupsCreated,
        passwords: result.passwordsStored,
      }));
      onClose();
    } catch (error) {
      setErrorCode(appErrorCode(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onOpenChange={(openState) => { if (!openState && !busy) onClose(); }}>
      <DialogContent className="sm:max-w-2xl" closeDisabled={busy} showClose={!busy}>
        <DialogHeader>
          <DialogTitle>{t(`${prefix}.title`)}</DialogTitle>
        </DialogHeader>
        <div className="space-y-2">
          <p className="text-sm text-[hsl(var(--muted))]">{t(`${prefix}.description`)}</p>
          {config.showTermiusTip && (
            <>
              <p className="text-sm text-[hsl(var(--muted))]">{t("terminusImport.termiusTip")}</p>
              <button
                type="button"
                className="inline-flex items-center gap-1.5 text-sm text-[hsl(var(--primary))] underline-offset-4 hover:underline"
                disabled={busy}
                onClick={() => void openUrl(TERMIUS_EXPORT_URL)}
              >
                <ExternalLink size={14} aria-hidden="true" />
                {t("terminusImport.termiusExportLink")}
              </button>
            </>
          )}
          {selectedWithPassword > 0 && (
            <p className="text-sm text-[hsl(var(--muted))]">{t("catalogImport.passwordVaultHint")}</p>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {config.allowFile && (
            <Button type="button" variant="secondary" size="sm" disabled={busy} onClick={() => void choosePath(false)}>
              {busy && !path ? <Loader2 className="animate-spin" size={15} aria-hidden="true" /> : <FileUp size={15} aria-hidden="true" />}
              {t("catalogImport.chooseFile")}
            </Button>
          )}
          {config.allowDirectory && (
            <Button type="button" variant="secondary" size="sm" disabled={busy} onClick={() => void choosePath(true)}>
              {busy && !path ? <Loader2 className="animate-spin" size={15} aria-hidden="true" /> : <FolderOpen size={15} aria-hidden="true" />}
              {t("catalogImport.chooseFolder")}
            </Button>
          )}
          {path && <span className="min-w-0 flex-1 truncate font-mono text-xs text-[hsl(var(--muted))]" title={path}>{path}</span>}
        </div>
        {profiles.length > 0 && (
          <div className="space-y-2">
            <div className="flex items-center justify-between gap-2 text-sm">
              <label className="inline-flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={selectedCount > 0 && selectedCount === importable.length}
                  disabled={busy || importable.length === 0}
                  onChange={(event) => toggleAll(event.target.checked)}
                />
                {t("catalogImport.selectAll", { count: importable.length })}
              </label>
              <span className="text-[hsl(var(--muted))]">{t("catalogImport.selectedCount", { count: selectedCount })}</span>
            </div>
            <ul className="max-h-[min(50vh,22rem)] space-y-2 overflow-y-auto rounded-lg border border-[hsl(var(--border))] p-2" aria-label={t("catalogImport.listLabel")}>
              {profiles.map((profile) => (
                <li key={profile.importKey} className={`rounded-md px-2 py-2 ${profile.importable ? "bg-[hsl(var(--elevated))]" : "opacity-60"}`}>
                  <label className="flex items-start gap-2">
                    <input
                      type="checkbox"
                      className="mt-1"
                      checked={selected.has(profile.importKey)}
                      disabled={busy || !profile.importable}
                      onChange={(event) => toggle(profile.importKey, event.target.checked)}
                    />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-medium">{profile.name}</span>
                      <span className="block truncate font-mono text-xs text-[hsl(var(--muted))]">
                        {profile.username}@{profile.host}{profile.port === 22 ? "" : `:${profile.port}`}
                        {profile.groupName ? ` · ${profile.groupName}` : ""}
                        {profile.hasPassword ? ` · ${t("catalogImport.hasPassword")}` : ""}
                      </span>
                      {profile.warnings.length > 0 && (
                        <span className="mt-1 flex flex-wrap gap-1">
                          {profile.warnings.map((warning) => (
                            <span key={warning} className="rounded bg-[hsl(var(--primary)/.1)] px-1.5 py-0.5 text-[11px] text-[hsl(var(--muted))]">
                              {t(`catalogImport.warnings.${warning}`, { defaultValue: t(`terminusImport.warnings.${warning}`, { defaultValue: warning }) })}
                            </span>
                          ))}
                        </span>
                      )}
                    </span>
                  </label>
                </li>
              ))}
            </ul>
          </div>
        )}
        {errorCode && (
          <p role="alert" className="text-sm text-red-500">
            {t(`errors.${errorCode}`, { defaultValue: t("errors.UNKNOWN") })}
          </p>
        )}
        <DialogFooter>
          <DialogClose asChild>
            <Button type="button" variant="secondary" disabled={busy}>{t("common.cancel")}</Button>
          </DialogClose>
          <Button type="button" disabled={busy || !path || selectedCount === 0} onClick={() => void apply()}>
            {busy && path ? <Loader2 className="animate-spin" size={15} aria-hidden="true" /> : null}
            {t("catalogImport.importSelected", { count: selectedCount })}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
