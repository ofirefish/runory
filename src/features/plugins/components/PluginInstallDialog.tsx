import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { DialogShell } from "../../../components/ui/dialog-shell";
import { pluginDiscardInstallReview, pluginInspectPackage, pluginInstall, pluginMarketplacePrepareInstall } from "../../../lib/tauri/plugins";
import { appErrorCode } from "../../../lib/app-error";
import type { MarketplacePlugin, PluginPackageInspection } from "../types/plugin";

export function PluginInstallDialog(props: {
  open: boolean;
  marketplacePlugin?: MarketplacePlugin | null;
  onClose: () => void;
  onInstalled: () => void;
}) {
  return props.open ? <InstallReview key={props.marketplacePlugin?.id ?? "local"} {...props} /> : null;
}

function InstallReview({ marketplacePlugin, onClose, onInstalled }: Omit<Parameters<typeof PluginInstallDialog>[0], "open">) {
  const { t } = useTranslation();
  const [inspection, setInspection] = useState<PluginPackageInspection | null>(null);
  const [granted, setGranted] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmed, setConfirmed] = useState(false);
  const active = useRef(false);
  const reviewId = useRef<string | null>(null);
  const pending = useRef(false);

  const acceptReview = (review: PluginPackageInspection) => {
    if (!active.current) {
      void pluginDiscardInstallReview(review.reviewId).catch(() => undefined);
      return;
    }
    reviewId.current = review.reviewId;
    setInspection(review);
    setGranted(review.manifest.permissions.map((permission) => typeof permission === "string" ? permission : permission.permission));
    setConfirmed(false);
  };

  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
      if (reviewId.current) void pluginDiscardInstallReview(reviewId.current).catch(() => undefined);
    };
  }, []);

  const prepare = async () => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    setInspection(null);
    setConfirmed(false);
    try {
      if (reviewId.current) {
        await pluginDiscardInstallReview(reviewId.current);
        reviewId.current = null;
      }
      if (marketplacePlugin) {
        acceptReview(await pluginMarketplacePrepareInstall(marketplacePlugin.id, marketplacePlugin.latestVersion));
      } else {
        const selected = await open({ multiple: false, directory: false,
          title: t("plugins.install.selectTitle"),
          filters: [{ name: t("plugins.install.packageType"), extensions: ["runory-plugin", "zip"] }],
        });
        if (typeof selected === "string") acceptReview(await pluginInspectPackage(selected));
      }
    } catch (cause) {
      if (active.current) setError(appErrorCode(cause));
    } finally {
      pending.current = false;
      if (active.current) setBusy(false);
    }
  };

  const confirmInstall = async () => {
    if (!inspection || !confirmed || pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    try {
      await pluginInstall({ reviewId: inspection.reviewId, grantedPermissions: granted });
      reviewId.current = null;
      onInstalled();
      onClose();
    } catch (cause) {
      if (active.current) {
        setError(appErrorCode(cause));
        setInspection(null);
        setConfirmed(false);
      }
    } finally {
      pending.current = false;
      if (active.current) setBusy(false);
    }
  };

  return (
    <DialogShell title={t("plugins.install.title")} onClose={onClose} closeDisabled={busy} size="form">
      <div className="space-y-4" aria-busy={busy}>
        <p className="text-sm text-muted-foreground">{t("plugins.install.hint")}</p>
        {marketplacePlugin && <p className="text-sm font-medium">{marketplacePlugin.name} · {marketplacePlugin.latestVersion}</p>}
        <Button type="button" onClick={() => void prepare()} disabled={busy}>
          {busy ? t("plugins.install.preparing") : marketplacePlugin ? t("plugins.install.inspectDownload") : t("plugins.install.select")}
        </Button>
        {inspection && (
          <div className="space-y-3 rounded-md border border-border p-3 text-sm">
            <p className="font-medium">{inspection.manifest.name}</p>
            <p className="break-words text-xs text-muted-foreground">
              {inspection.manifest.id} · {inspection.manifest.version} · {inspection.manifest.publisher.name}
            </p>
            <p>{t(`plugins.trust.${inspection.trust}`)}</p>
            {inspection.installedVersion && <p>{t("plugins.updates.versionChange", { from: inspection.installedVersion, to: inspection.manifest.version })}</p>}
            <fieldset disabled={busy} className="space-y-2">
              <legend>{t("plugins.install.permissions")}</legend>
              {inspection.manifest.permissions.length === 0 && <p>{t("plugins.detail.noPermissions")}</p>}
              {inspection.manifest.permissions.map((item) => {
                const permission = typeof item === "string" ? item : item.permission;
                const hosts = typeof item === "string" ? [] : item.hosts ?? [];
                return <label key={permission} className="flex items-start gap-2 break-all text-xs">
                  <input type="checkbox" checked={granted.includes(permission)} onChange={(event) => {
                    setGranted((current) => event.target.checked ? [...current, permission] : current.filter((value) => value !== permission));
                    setConfirmed(false);
                  }} />
                  <span><code>{permission}</code>{inspection.installedVersion && inspection.permissionAdditions.includes(permission) ? ` · ${t("plugins.install.expandedPermission")}` : ""}
                    <span className="block text-muted-foreground">{hosts.length ? t("plugins.install.hosts", { hosts: hosts.join(", ") }) : t("plugins.install.allHosts")}</span>
                  </span>
                </label>;
              })}
            </fieldset>
            <p className="text-xs text-muted-foreground">{t("plugins.install.deniedHint")}</p>
            <label className="flex items-start gap-2">
              <input type="checkbox" disabled={busy} checked={confirmed} onChange={(event) => setConfirmed(event.target.checked)} />
              {t("plugins.install.reviewConfirmation")}
            </label>
            <Button type="button" onClick={() => void confirmInstall()} disabled={busy || !confirmed}>
              {t(inspection.installedVersion ? "plugins.updates.confirm" : "plugins.install.confirm")}
            </Button>
          </div>
        )}
        {error && <p role="alert" className="text-sm text-destructive">{t(`errors.${error}`, { defaultValue: t("errors.UNKNOWN") })}</p>}
      </div>
    </DialogShell>
  );
}
