import { useTranslation } from "react-i18next";
import { BadgeCheck } from "lucide-react";
import { Button } from "../../../components/ui/button";
import type { MarketplacePlugin, PluginDto } from "../types/plugin";
import { PluginLogo } from "../workspace/PluginLogo";
import { PluginIcon } from "./PluginIcon";

type DetailSource =
  | { kind: "installed"; plugin: PluginDto }
  | { kind: "marketplace"; plugin: MarketplacePlugin; installed: boolean };

export function PluginDetailPane({
  source,
  installing,
  installFailed,
  onInstall,
  onToggle,
  onUninstall,
  busy,
}: {
  source: DetailSource | null;
  installing?: boolean;
  installFailed?: boolean;
  onInstall?: () => void;
  onToggle?: (enabled: boolean) => void;
  onUninstall?: () => void;
  busy?: boolean;
}) {
  const { t } = useTranslation();

  if (!source) {
    return (
      <div className="plugin-detail plugin-detail-empty">
        <p>{t("plugins.detail.select")}</p>
      </div>
    );
  }

  const { plugin } = source;
  const isInstalled = source.kind === "installed";
  const marketplaceInstalled = source.kind === "marketplace" && source.installed;
  const version = isInstalled ? source.plugin.version : source.plugin.latestVersion;
  const description = plugin.description;
  const verified = source.kind === "marketplace" ? source.plugin.verified : false;

  return (
    <div className="plugin-detail">
      <header className="plugin-detail-header">
        {isInstalled && source.plugin.logoUrl ? (
          <PluginLogo title={plugin.name} logoUrl={source.plugin.logoUrl} size="lg" />
        ) : (
          <PluginIcon id={plugin.id} name={plugin.name} size="lg" />
        )}
        <div className="plugin-detail-heading">
          <h2>{plugin.name}</h2>
          <p className="plugin-detail-publisher">
            {plugin.publisherId}
            {verified ? (
              <>
                <span aria-hidden="true"> · </span>
                <span className="plugin-detail-verified">
                  <BadgeCheck size={14} aria-hidden="true" />
                  {t("plugins.marketplace.verified")}
                </span>
              </>
            ) : null}
          </p>
          <p className="plugin-detail-id">{plugin.id}</p>
        </div>
        <div className="plugin-detail-actions">
          {source.kind === "marketplace" && !marketplaceInstalled ? (
            <Button type="button" size="sm" disabled={installing} onClick={onInstall}>
              {installing ? t("plugins.marketplace.installing") : source.plugin.updateAvailable ? t("plugins.updates.review") : t("plugins.marketplace.install")}
            </Button>
          ) : null}
          {source.kind === "marketplace" && marketplaceInstalled ? (
            <Button type="button" size="sm" variant="secondary" disabled>
              {t("plugins.marketplace.installed")}
            </Button>
          ) : null}
          {isInstalled ? (
            <>
              <Button
                type="button"
                size="sm"
                variant="secondary"
                disabled={busy}
                onClick={() => onToggle?.(!source.plugin.enabled)}
              >
                {source.plugin.enabled ? t("plugins.action.disable") : t("plugins.action.enable")}
              </Button>
              <Button type="button" size="sm" variant="danger" disabled={busy} onClick={onUninstall}>
                {t("plugins.action.uninstall")}
              </Button>
            </>
          ) : null}
        </div>
      </header>

      {installFailed ? (
        <p className="plugin-detail-error">{t("plugins.marketplace.installFailed")}</p>
      ) : null}

      <dl className="plugin-detail-facts">
        <div>
          <dt>{t("plugins.detail.version")}</dt>
          <dd>v{version}</dd>
        </div>
        {source.kind === "marketplace" && source.plugin.category ? (
          <div>
            <dt>{t("plugins.detail.category")}</dt>
            <dd>{source.plugin.category}</dd>
          </div>
        ) : null}
        {isInstalled ? (
          <div>
            <dt>{t("plugins.detail.workspaceCategory")}</dt>
            <dd>
              {source.plugin.category
                ? t(`plugins.workspace.category.${source.plugin.category}`)
                : t("plugins.workspace.category.none")}
            </dd>
          </div>
        ) : null}
        {isInstalled ? (
          <div>
            <dt>{t("plugins.detail.status")}</dt>
            <dd>
              {source.plugin.enabled ? t("plugins.status.enabled") : t("plugins.status.disabled")}
            </dd>
          </div>
        ) : null}
      </dl>

      <section className="plugin-detail-section">
        <h3>{t("plugins.detail.description")}</h3>
        <p>{description?.trim() || t("plugins.detail.noDescription")}</p>
      </section>

      {isInstalled ? (
        <>
          <section className="plugin-detail-section">
            <h3>{t("plugins.install.permissions")}</h3>
            {source.plugin.permissions.length === 0 ? (
              <p className="plugin-detail-muted">{t("plugins.detail.noPermissions")}</p>
            ) : (
              <ul className="plugin-detail-list">
                {source.plugin.permissions.map((permission) => (
                  <li key={permission.permission}>
                    <code>{permission.permission}</code>
                    {!permission.granted ? (
                      <span className="plugin-detail-denied">
                        {" "}
                        ({t("plugins.permission.denied")})
                      </span>
                    ) : null}
                  </li>
                ))}
              </ul>
            )}
          </section>

          {source.plugin.commands.length > 0 ? (
            <section className="plugin-detail-section">
              <h3>{t("plugins.detail.commands")}</h3>
              <ul className="plugin-detail-list">
                {source.plugin.commands.map((command) => (
                  <li key={command.id}>
                    {command.title}
                    <span className="plugin-detail-muted"> · {command.id}</span>
                  </li>
                ))}
              </ul>
            </section>
          ) : null}

          {source.plugin.workflows.length > 0 ? (
            <section className="plugin-detail-section">
              <h3>{t("plugins.detail.workflows")}</h3>
              <ul className="plugin-detail-list">
                {source.plugin.workflows.map((workflow) => (
                  <li key={workflow.id}>
                    <code>{workflow.id}</code>
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
        </>
      ) : null}
    </div>
  );
}
