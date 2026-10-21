import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  ChevronDown,
  ChevronRight,
  Search,
  PackagePlus,
  Code2,
  Puzzle,
  ShieldCheck,
} from "lucide-react";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { getSettings, updateSettings } from "../../../lib/tauri/settings";
import {
  pluginMarketplaceList,
  pluginMarketplaceSyncRevocations,
} from "../../../lib/tauri/plugins";
import { appErrorCode } from "../../../lib/app-error";
import { DialogShell } from "../../../components/ui/dialog-shell";
import type { MarketplacePlugin } from "../types/plugin";
import { usePluginStore } from "../store/pluginStore";
import { PluginDetailPane } from "./PluginDetailPane";
import { PluginInstallDialog } from "./PluginInstallDialog";
import { PluginListItem } from "./PluginListItem";

type SectionId = "discover" | "installed" | "updates";

type Selection = {
  section: SectionId;
  id: string;
};

function PluginSection({
  id,
  title,
  count,
  open,
  onToggle,
  children,
}: {
  id: SectionId;
  title: string;
  count?: number;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <section className="plugins-hub-group" data-section={id}>
      <button
        type="button"
        className="plugins-hub-group-header"
        aria-expanded={open}
        onClick={onToggle}
      >
        {open ? <ChevronDown size={14} aria-hidden="true" /> : <ChevronRight size={14} aria-hidden="true" />}
        <span className="plugins-hub-group-title">{title}</span>
        {typeof count === "number" ? <span className="plugins-hub-group-count">{count}</span> : null}
      </button>
      {open ? <div className="plugins-hub-group-body">{children}</div> : null}
    </section>
  );
}

export function PluginsHub({
  initialTab = "installed",
  variant = "embedded",
}: {
  initialTab?: SectionId;
  variant?: "embedded" | "page";
}) {
  const { t } = useTranslation();
  const { plugins, loading, error, refresh, setEnabled, uninstall } = usePluginStore();
  const [query, setQuery] = useState("");
  const [selection, setSelection] = useState<Selection | null>(null);
  const [installOpen, setInstallOpen] = useState(false);
  const [marketplace, setMarketplace] = useState<MarketplacePlugin[]>([]);
  const [developerMode, setDeveloperMode] = useState(false);
  const [marketError, setMarketError] = useState(false);
  const [installTarget, setInstallTarget] = useState<MarketplacePlugin | null>(null);
  const [marketLoading, setMarketLoading] = useState(true);
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionBusy, setActionBusy] = useState(false);
  const actionPending = useRef(false);
  const marketRequest = useRef(0);
  const [uninstallTarget, setUninstallTarget] = useState<string | null>(null);
  const [openSections, setOpenSections] = useState<Record<SectionId, boolean>>({
    discover: initialTab === "discover",
    installed: initialTab === "installed" || initialTab !== "updates",
    updates: initialTab === "updates",
  });

  useEffect(() => {
    setOpenSections((current) => ({
      ...current,
      [initialTab]: true,
    }));
  }, [initialTab]);

  const refreshMarketplace = useCallback(async () => {
    const request = ++marketRequest.current;
    setMarketLoading(true);
    setMarketError(false);
    try {
      await pluginMarketplaceSyncRevocations();
    } catch { /* Offline local management remains available. */ }
    try {
      const catalog = await pluginMarketplaceList();
      if (request === marketRequest.current) setMarketplace(catalog);
    } catch {
      if (request === marketRequest.current) { setMarketplace([]); setMarketError(true); }
    } finally {
      if (request === marketRequest.current) setMarketLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    void getSettings()
      .then((settings) => setDeveloperMode(Boolean(settings.pluginDeveloperMode)))
      .catch(() => undefined);
    void refreshMarketplace();
    return () => { marketRequest.current += 1; };
  }, [refresh, refreshMarketplace]);

  const normalizedQuery = query.trim().toLowerCase();

  const filteredMarketplace = useMemo(() => {
    if (!normalizedQuery) return marketplace;
    return marketplace.filter((plugin) =>
      [plugin.name, plugin.id, plugin.publisherId, plugin.description ?? ""]
        .join(" ")
        .toLowerCase()
        .includes(normalizedQuery),
    );
  }, [marketplace, normalizedQuery]);

  const filteredInstalled = useMemo(() => {
    if (!normalizedQuery) return plugins;
    return plugins.filter((plugin) =>
      [plugin.name, plugin.id, plugin.publisherId].join(" ").toLowerCase().includes(normalizedQuery),
    );
  }, [plugins, normalizedQuery]);

  const updates = useMemo(() => filteredMarketplace.filter((plugin) => plugin.updateAvailable
    && plugins.some((installed) => installed.id === plugin.id && installed.version === plugin.installedVersion)), [filteredMarketplace, plugins]);

  const selectable = useMemo(() => {
    const items: Selection[] = [];
    if (openSections.updates) {
      for (const plugin of updates) items.push({ section: "updates", id: plugin.id });
    }
    if (openSections.installed) {
      for (const plugin of filteredInstalled) {
        items.push({ section: "installed", id: plugin.id });
      }
    }
    if (openSections.discover) {
      for (const plugin of filteredMarketplace) {
        items.push({ section: "discover", id: plugin.id });
      }
    }
    return items;
  }, [openSections, filteredInstalled, filteredMarketplace, updates]);

  const selectableKey = selectable.map((item) => `${item.section}:${item.id}`).join("\0");

  useEffect(() => {
    if (selectable.length === 0) {
      setSelection(null);
      return;
    }
    setSelection((current) => {
      if (
        current &&
        selectable.some((item) => item.section === current.section && item.id === current.id)
      ) {
        return current;
      }
      return selectable.find((item) => item.section === initialTab) ?? selectable[0]!;
    });
  }, [selectableKey, selectable, initialTab]);

  const toggleSection = (id: SectionId) => {
    setOpenSections((current) => ({ ...current, [id]: !current[id] }));
  };

  const performAction = async (action: () => Promise<void>) => {
    if (actionPending.current) return;
    actionPending.current = true;
    setActionBusy(true);
    setActionError(null);
    try {
      await action();
    } catch (cause) {
      setActionError(appErrorCode(cause));
    } finally {
      actionPending.current = false;
      setActionBusy(false);
    }
  };

  const toggleDeveloperMode = () => performAction(async () => {
    const next = !developerMode;
    await updateSettings({ pluginDeveloperMode: next });
    setDeveloperMode(next);
  });

  const installFromMarketplace = (plugin: MarketplacePlugin) => {
    setInstallTarget(plugin);
    setInstallOpen(true);
  };

  const isInstalled = (pluginId: string) => plugins.some((p) => p.id === pluginId);

  const selectedMarketplace =
    selection?.section === "discover" || selection?.section === "updates"
      ? (filteredMarketplace.find((plugin) => plugin.id === selection.id) ?? null)
      : null;
  const selectedInstalled =
    selection?.section === "installed"
      ? (filteredInstalled.find((plugin) => plugin.id === selection.id) ?? null)
      : null;

  const detailSource = selectedInstalled
    ? ({ kind: "installed" as const, plugin: selectedInstalled })
    : selectedMarketplace
      ? ({
          kind: "marketplace" as const,
          plugin: selectedMarketplace,
          installed: isInstalled(selectedMarketplace.id) && !selectedMarketplace.updateAvailable,
        })
      : null;

  const actionButtons = (
    <>
      <Button
        type="button"
        size="sm"
        variant="secondary"
        disabled={actionBusy}
        onClick={() => void toggleDeveloperMode()}
        title={developerMode ? t("plugins.developer.on") : t("plugins.developer.off")}
      >
        <Code2 size={14} aria-hidden="true" />
        <span className="plugins-hub-action-label">
          {developerMode ? t("plugins.developer.onShort") : t("plugins.developer.offShort")}
        </span>
      </Button>
      <Button type="button" size="sm" onClick={() => { setInstallTarget(null); setInstallOpen(true); }}>
        <PackagePlus size={15} aria-hidden="true" />
        <span className="plugins-hub-action-label">{t("plugins.install.openShort")}</span>
      </Button>
      <Button type="button" size="sm" variant="secondary" disabled={marketLoading || loading}
        onClick={() => { void refresh(); void refreshMarketplace(); }}>{t("plugins.action.refresh")}</Button>
    </>
  );

  return (
    <section className={`plugins-hub${variant === "page" ? " plugins-hub-page" : ""}`}>
      {variant === "page" ? (
        <header className="plugins-header">
          <div className="plugins-heading">
            <span className="plugins-heading-icon">
              <Puzzle size={20} aria-hidden="true" />
            </span>
            <div>
              <h1>{t("plugins.page.title")}</h1>
              <p>{t("plugins.page.subtitle")}</p>
            </div>
          </div>
          <span className="plugins-scope">
            <ShieldCheck size={14} aria-hidden="true" />
            {t("plugins.page.trustHint")}
          </span>
          <div className="plugins-header-actions">{actionButtons}</div>
        </header>
      ) : null}

      <div className="plugins-hub-split">
        <aside className="plugins-hub-list" aria-label={t("plugins.hub.title")}>
          <div className="plugins-hub-sidebar-top">
            <div className="plugins-hub-search">
              <Search size={14} aria-hidden="true" />
              <Input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={t("plugins.search.placeholder")}
                aria-label={t("plugins.search.placeholder")}
              />
            </div>
            {variant === "embedded" ? (
              <div className="plugins-hub-actions">{actionButtons}</div>
            ) : null}
          </div>

          <div className="plugins-hub-groups">
            <PluginSection
              id="installed"
              title={t("plugins.tab.installed")}
              count={filteredInstalled.length}
              open={openSections.installed}
              onToggle={() => toggleSection("installed")}
            >
              {loading && <p className="plugins-hub-empty">{t("plugins.loading")}</p>}
              {error && <p className="plugins-hub-empty is-error">{t("errors.loadFailed")}</p>}
              {!loading && filteredInstalled.length === 0 && (
                <p className="plugins-hub-empty">
                  {normalizedQuery ? t("plugins.search.empty") : t("plugins.installed.empty")}
                </p>
              )}
              <div role="listbox" aria-label={t("plugins.tab.installed")}>
                {filteredInstalled.map((plugin) => (
                  <PluginListItem
                    key={`installed:${plugin.id}@${plugin.version}`}
                    entry={{
                      id: plugin.id,
                      name: plugin.name,
                      publisherId: plugin.publisherId,
                      version: plugin.version,
                      enabled: plugin.enabled,
                      installed: true,
                    }}
                    selected={selection?.section === "installed" && selection.id === plugin.id}
                    onSelect={() => setSelection({ section: "installed", id: plugin.id })}
                  />
                ))}
              </div>
            </PluginSection>

            <PluginSection
              id="discover"
              title={t("plugins.tab.discover")}
              count={filteredMarketplace.length}
              open={openSections.discover}
              onToggle={() => toggleSection("discover")}
            >
              {marketError && (
                <p className="plugins-hub-empty">{t("plugins.marketplace.unavailable")}</p>
              )}
              {marketLoading && <p className="plugins-hub-empty" role="status">{t("plugins.loading")}</p>}
              {!marketLoading && !marketError && filteredMarketplace.length === 0 && (
                <p className="plugins-hub-empty">
                  {normalizedQuery ? t("plugins.search.empty") : t("plugins.marketplace.empty")}
                </p>
              )}
              <div role="listbox" aria-label={t("plugins.tab.discover")}>
                {filteredMarketplace.map((plugin) => {
                  const installed = isInstalled(plugin.id);
                  return (
                    <PluginListItem
                      key={`discover:${plugin.id}`}
                      entry={{
                        id: plugin.id,
                        name: plugin.name,
                        publisherId: plugin.publisherId,
                        version: plugin.latestVersion,
                        description: plugin.description,
                        verified: plugin.verified,
                        installed,
                      }}
                      selected={selection?.section === "discover" && selection.id === plugin.id}
                      onSelect={() => setSelection({ section: "discover", id: plugin.id })}
                      actionLabel={
                        plugin.updateAvailable ? t("plugins.updates.review") : installed
                          ? t("plugins.marketplace.installed")
                          : t("plugins.marketplace.install")
                      }
                      actionDisabled={installed && !plugin.updateAvailable}
                      onAction={() => void installFromMarketplace(plugin)}
                    />
                  );
                })}
              </div>
            </PluginSection>

            <PluginSection
              id="updates"
              title={t("plugins.tab.updates")}
              count={updates.length}
              open={openSections.updates}
              onToggle={() => toggleSection("updates")}
            >
              {marketLoading ? <p className="plugins-hub-empty" role="status">{t("plugins.loading")}</p>
                : marketError ? <p className="plugins-hub-empty">{t("plugins.marketplace.unavailable")}</p>
                : updates.length === 0 ? <p className="plugins-hub-empty">{t(normalizedQuery ? "plugins.search.empty" : "plugins.updates.empty")}</p> : null}
              <div role="listbox" aria-label={t("plugins.tab.updates")}>
                {updates.map((plugin) => <PluginListItem key={`update:${plugin.id}`} entry={{
                  id: plugin.id, name: plugin.name, publisherId: plugin.publisherId,
                  version: plugin.latestVersion, description: t("plugins.updates.versionChange", { from: plugin.installedVersion, to: plugin.latestVersion }),
                }} selected={selection?.section === "updates" && selection.id === plugin.id}
                  onSelect={() => setSelection({ section: "updates", id: plugin.id })}
                  actionLabel={t("plugins.updates.review")} onAction={() => installFromMarketplace(plugin)} />)}
              </div>
            </PluginSection>
          </div>
        </aside>

        <div className="plugins-hub-detail">
          {actionError && <p role="alert" className="plugin-detail-error">{t(`errors.${actionError}`, { defaultValue: t("errors.UNKNOWN") })}</p>}
          <PluginDetailPane
            source={detailSource}
            busy={actionBusy}
            onInstall={
              selectedMarketplace
                ? () => void installFromMarketplace(selectedMarketplace)
                : undefined
            }
            onToggle={
              selectedInstalled
                ? (enabled) => void performAction(() => setEnabled(selectedInstalled.id, enabled))
                : undefined
            }
            onUninstall={
              selectedInstalled ? () => setUninstallTarget(selectedInstalled.id) : undefined
            }
          />
        </div>
      </div>

      <PluginInstallDialog
        open={installOpen}
        marketplacePlugin={installTarget}
        onClose={() => setInstallOpen(false)}
        onInstalled={() => { void refresh(); void refreshMarketplace(); setOpenSections((current) => ({ ...current, installed: true })); }}
      />
      {uninstallTarget && <DialogShell title={t("plugins.action.uninstall")} onClose={() => setUninstallTarget(null)} closeDisabled={actionBusy}>
        <p className="mb-4 text-sm">{t("plugins.uninstall.confirm", { name: plugins.find((plugin) => plugin.id === uninstallTarget)?.name ?? uninstallTarget })}</p>
        <Button variant="danger" disabled={actionBusy} onClick={() => void performAction(async () => {
          await uninstall(uninstallTarget); setUninstallTarget(null); void refreshMarketplace();
        })}>{t("plugins.action.uninstall")}</Button>
        {actionError && <p role="alert" className="text-sm text-destructive">{t(`errors.${actionError}`, { defaultValue: t("errors.UNKNOWN") })}</p>}
      </DialogShell>}
    </section>
  );
}
