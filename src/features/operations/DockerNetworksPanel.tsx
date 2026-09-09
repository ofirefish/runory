import { Search, Settings, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import { SelectControl } from "../../components/ui/select-control";
import type { DockerNetwork, DockerNetworkAction } from "../../types/infrastructure";
import { cn } from "../../lib/utils";

const PAGE_SIZE_OPTIONS = ["10", "20", "50", "100"] as const;
const DRIVER_OPTIONS = ["bridge", "overlay", "macvlan", "ipvlan"] as const;
const BUILTIN_NETWORKS = new Set(["bridge", "host", "none"]);

function formatCreatedAt(epochSeconds: number, locale: string): string {
  if (!epochSeconds) return "—";
  return new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(new Date(epochSeconds * 1000));
}

function parseLabels(value: string): string[] {
  return value
    .split(/[\s,]+/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function isBuiltinNetwork(name: string): boolean {
  return BUILTIN_NETWORKS.has(name);
}

type ConfirmKind =
  | { kind: "remove"; ids: string[]; label: string }
  | { kind: "prune" }
  | { kind: "bulkRemove"; ids: string[] };

export function DockerNetworksPanel({
  networks,
  busy = false,
  onAction,
  onOpenSettings,
}: {
  networks: DockerNetwork[];
  busy?: boolean;
  onAction: (action: DockerNetworkAction) => Promise<void>;
  onOpenSettings?: () => void;
}) {
  const { t, i18n } = useTranslation();
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState<(typeof PAGE_SIZE_OPTIONS)[number]>("20");
  const [jumpPage, setJumpPage] = useState("1");
  const [bulkAction, setBulkAction] = useState<"none" | "remove">("none");
  const [createOpen, setCreateOpen] = useState(false);
  const [createName, setCreateName] = useState("");
  const [createDriver, setCreateDriver] = useState<(typeof DRIVER_OPTIONS)[number]>("bridge");
  const [createSubnet, setCreateSubnet] = useState("");
  const [createGateway, setCreateGateway] = useState("");
  const [createLabels, setCreateLabels] = useState("");
  const [confirm, setConfirm] = useState<ConfirmKind | null>(null);
  const [actionBusy, setActionBusy] = useState(false);
  const [actionFailed, setActionFailed] = useState(false);
  const [actionErrorDetail, setActionErrorDetail] = useState("");

  const networkList = networks ?? [];

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return networkList;
    return networkList.filter((network) => {
      const haystack = [
        network.id,
        network.name,
        network.driver,
        network.ipv4Subnet,
        network.ipv4Gateway,
        network.labels,
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(needle);
    });
  }, [networkList, query]);

  const pageSizeNumber = Number(pageSize);
  const totalPages = Math.max(1, Math.ceil(filtered.length / pageSizeNumber));
  const currentPage = Math.min(page, totalPages);
  const pageItems = filtered.slice((currentPage - 1) * pageSizeNumber, currentPage * pageSizeNumber);
  const selectablePageItems = pageItems.filter((item) => !isBuiltinNetwork(item.name));
  const allPageSelected =
    selectablePageItems.length > 0 && selectablePageItems.every((item) => selected.has(item.id));

  const pageNumbers = useMemo(() => {
    const windowSize = 5;
    const start = Math.max(1, Math.min(currentPage - 2, totalPages - windowSize + 1));
    const end = Math.min(totalPages, start + windowSize - 1);
    return Array.from({ length: end - start + 1 }, (_, index) => start + index);
  }, [currentPage, totalPages]);

  useEffect(() => {
    setPage(1);
  }, [query, pageSize]);

  useEffect(() => {
    setJumpPage(String(currentPage));
  }, [currentPage]);

  useEffect(() => {
    const valid = new Set(networkList.map((network) => network.id));
    setSelected((previous) => {
      const next = new Set(
        [...previous].filter((id) => {
          if (!valid.has(id)) return false;
          const network = networkList.find((item) => item.id === id);
          return network ? !isBuiltinNetwork(network.name) : false;
        }),
      );
      return next.size === previous.size ? previous : next;
    });
  }, [networkList]);

  const toggleAllPage = () => {
    setSelected((previous) => {
      const next = new Set(previous);
      if (allPageSelected) {
        for (const item of selectablePageItems) next.delete(item.id);
      } else {
        for (const item of selectablePageItems) next.add(item.id);
      }
      return next;
    });
  };

  const toggleOne = (id: string) => {
    setSelected((previous) => {
      const next = new Set(previous);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const runAction = async (action: DockerNetworkAction) => {
    setActionBusy(true);
    setActionFailed(false);
    setActionErrorDetail("");
    try {
      await onAction(action);
      setCreateOpen(false);
      setCreateName("");
      setCreateDriver("bridge");
      setCreateSubnet("");
      setCreateGateway("");
      setCreateLabels("");
      setConfirm(null);
      if (action.type === "remove") {
        setSelected((previous) => {
          const next = new Set(previous);
          for (const id of action.ids) next.delete(id);
          return next;
        });
        toast.success(
          action.ids.length > 1
            ? t("operations.dockerNetworks.deleteSuccessBulk", { count: action.ids.length })
            : t("operations.dockerNetworks.deleteSuccess"),
        );
      } else if (action.type === "prune") {
        toast.success(t("operations.dockerNetworks.pruneSuccess"));
      } else if (action.type === "create") {
        toast.success(t("operations.dockerNetworks.createSuccess"));
      }
    } catch (error) {
      setActionFailed(true);
      setActionErrorDetail(error instanceof Error ? error.message : String(error));
    } finally {
      setActionBusy(false);
    }
  };

  const failureMessage = (
    <div className="space-y-1">
      <p className="text-sm text-red-500">{t("operations.error")}</p>
      {actionErrorDetail ? (
        <pre className="max-h-32 overflow-auto whitespace-pre-wrap rounded-md border border-red-500/20 bg-red-500/5 p-2 font-mono text-xs text-red-500/90">
          {actionErrorDetail.slice(0, 2_000)}
        </pre>
      ) : null}
    </div>
  );

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <Button
          type="button"
          size="sm"
          disabled={busy || actionBusy}
          onClick={() => {
            setActionFailed(false);
            setCreateOpen(true);
          }}
        >
          {t("operations.dockerNetworks.create")}
        </Button>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={busy || actionBusy}
          onClick={() => {
            setActionFailed(false);
            setConfirm({ kind: "prune" });
          }}
        >
          {t("operations.dockerNetworks.prune")}
        </Button>
        <div className="relative ml-auto min-w-[16rem] flex-1 sm:max-w-sm">
          <Search size={14} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[hsl(var(--muted))]" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("operations.dockerNetworks.searchPlaceholder")}
            aria-label={t("operations.dockerNetworks.searchPlaceholder")}
            className="pl-8"
          />
        </div>
        {onOpenSettings ? (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            title={t("operations.dockerTab.settings")}
            aria-label={t("operations.dockerTab.settings")}
            onClick={onOpenSettings}
          >
            <Settings size={15} />
          </Button>
        ) : null}
      </div>

      <div className="min-h-0 flex-1 overflow-auto rounded-lg border">
        <table className="w-full text-left text-sm">
          <thead className="sticky top-0 bg-[hsl(var(--elevated))] text-xs">
            <tr>
              <th className="w-10 px-3 py-2">
                <input
                  type="checkbox"
                  checked={allPageSelected}
                  onChange={toggleAllPage}
                  aria-label={t("operations.dockerNetworks.selectAll")}
                />
              </th>
              <th className="px-3 py-2">{t("operations.dockerNetworks.name")}</th>
              <th className="px-3 py-2">{t("operations.dockerNetworks.driver")}</th>
              <th className="px-3 py-2">{t("operations.dockerNetworks.subnet")}</th>
              <th className="px-3 py-2">{t("operations.dockerNetworks.gateway")}</th>
              <th className="px-3 py-2">{t("operations.dockerNetworks.labels")}</th>
              <th className="px-3 py-2">{t("operations.dockerNetworks.createdAt")}</th>
              <th className="w-20 px-3 py-2">{t("operations.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {pageItems.length === 0 ? (
              <tr>
                <td colSpan={8} className="px-3 py-8 text-center text-[hsl(var(--muted))]">
                  {t("operations.dockerNetworks.empty")}
                </td>
              </tr>
            ) : (
              pageItems.map((network) => {
                const builtin = isBuiltinNetwork(network.name);
                return (
                  <tr key={network.id} className="border-t hover:bg-[hsl(var(--elevated)/.45)]">
                    <td className="px-3 py-2">
                      <input
                        type="checkbox"
                        checked={selected.has(network.id)}
                        disabled={builtin}
                        onChange={() => toggleOne(network.id)}
                        aria-label={t("operations.dockerNetworks.selectRow", { name: network.name })}
                      />
                    </td>
                    <td className="max-w-xs truncate px-3 py-2 font-mono text-xs" title={network.name}>
                      {network.name}
                    </td>
                    <td className="px-3 py-2 font-mono text-xs">{network.driver || "—"}</td>
                    <td className="px-3 py-2 font-mono text-xs">{network.ipv4Subnet || "—"}</td>
                    <td className="px-3 py-2 font-mono text-xs">{network.ipv4Gateway || "—"}</td>
                    <td className="max-w-xs truncate px-3 py-2 text-xs" title={network.labels || undefined}>
                      {network.labels || "—"}
                    </td>
                    <td className="whitespace-nowrap px-3 py-2 tabular-nums">
                      {formatCreatedAt(network.createdAtEpochSeconds, i18n.language)}
                    </td>
                    <td className="px-3 py-2">
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 text-red-500 hover:text-red-500"
                        title={t("operations.dockerNetworks.delete")}
                        aria-label={t("operations.dockerNetworks.delete")}
                        disabled={busy || actionBusy || builtin}
                        onClick={() => {
                          setActionFailed(false);
                          setConfirm({ kind: "remove", ids: [network.id], label: network.name });
                        }}
                      >
                        <Trash2 size={13} />
                      </Button>
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <SelectControl
            value={bulkAction}
            onValueChange={setBulkAction}
            label={t("operations.dockerNetworks.bulkAction")}
            className="w-44"
            options={[
              { value: "none", label: t("operations.dockerNetworks.bulkPlaceholder") },
              { value: "remove", label: t("operations.dockerNetworks.bulkDelete") },
            ]}
          />
          <Button
            type="button"
            size="sm"
            disabled={busy || actionBusy || bulkAction === "none" || selected.size === 0}
            onClick={() => {
              if (bulkAction !== "remove") return;
              setActionFailed(false);
              setConfirm({ kind: "bulkRemove", ids: [...selected] });
            }}
          >
            {t("operations.dockerNetworks.bulkRun")}
          </Button>
        </div>

        <div className="ml-auto flex flex-wrap items-center gap-2 text-xs text-[hsl(var(--secondary))]">
          <Button type="button" variant="ghost" size="sm" disabled={currentPage <= 1} onClick={() => setPage(currentPage - 1)}>
            ‹
          </Button>
          {pageNumbers.map((value) => (
            <Button
              key={value}
              type="button"
              variant={value === currentPage ? "secondary" : "ghost"}
              size="sm"
              className={cn("min-w-8 px-2", value === currentPage && "font-semibold")}
              onClick={() => setPage(value)}
            >
              {value}
            </Button>
          ))}
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={currentPage >= totalPages}
            onClick={() => setPage(currentPage + 1)}
          >
            ›
          </Button>
          <SelectControl
            value={pageSize}
            onValueChange={setPageSize}
            label={t("operations.dockerNetworks.pageSize")}
            className="w-28"
            options={PAGE_SIZE_OPTIONS.map((value) => ({
              value,
              label: t("operations.dockerNetworks.pageSizeOption", { count: Number(value) }),
            }))}
          />
          <span>{t("operations.dockerNetworks.total", { count: filtered.length })}</span>
          <span className="inline-flex items-center gap-1">
            {t("operations.dockerNetworks.goto")}
            <Input
              type="number"
              min={1}
              max={totalPages}
              value={jumpPage}
              onChange={(event) => setJumpPage(event.target.value)}
              onKeyDown={(event) => {
                if (event.key !== "Enter") return;
                const next = Number(jumpPage);
                if (Number.isFinite(next)) setPage(Math.min(totalPages, Math.max(1, Math.trunc(next))));
              }}
              className="h-8 w-14 px-2"
              aria-label={t("operations.dockerNetworks.goto")}
            />
            {t("operations.dockerNetworks.page")}
          </span>
        </div>
      </div>

      {createOpen && (
        <DialogShell
          title={t("operations.dockerNetworks.createTitle")}
          onClose={() => !actionBusy && setCreateOpen(false)}
          closeDisabled={actionBusy}
          size="form"
        >
          <div className="space-y-3">
            <div className="space-y-1.5">
              <Label htmlFor="docker-network-name">{t("operations.dockerNetworks.name")}</Label>
              <Input
                id="docker-network-name"
                value={createName}
                placeholder={t("operations.dockerNetworks.namePlaceholder")}
                onChange={(event) => setCreateName(event.target.value)}
                disabled={actionBusy}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="docker-network-driver">{t("operations.dockerNetworks.driver")}</Label>
              <SelectControl
                id="docker-network-driver"
                value={createDriver}
                onValueChange={setCreateDriver}
                label={t("operations.dockerNetworks.driver")}
                className="w-full"
                options={DRIVER_OPTIONS.map((value) => ({ value, label: value }))}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="docker-network-subnet">{t("operations.dockerNetworks.subnet")}</Label>
              <Input
                id="docker-network-subnet"
                value={createSubnet}
                placeholder={t("operations.dockerNetworks.subnetPlaceholder")}
                onChange={(event) => setCreateSubnet(event.target.value)}
                disabled={actionBusy}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="docker-network-gateway">{t("operations.dockerNetworks.gateway")}</Label>
              <Input
                id="docker-network-gateway"
                value={createGateway}
                placeholder={t("operations.dockerNetworks.gatewayPlaceholder")}
                onChange={(event) => setCreateGateway(event.target.value)}
                disabled={actionBusy}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="docker-network-labels">{t("operations.dockerNetworks.labels")}</Label>
              <Input
                id="docker-network-labels"
                value={createLabels}
                placeholder={t("operations.dockerNetworks.labelsPlaceholder")}
                onChange={(event) => setCreateLabels(event.target.value)}
                disabled={actionBusy}
              />
            </div>
            {actionFailed && failureMessage}
            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" disabled={actionBusy} onClick={() => setCreateOpen(false)}>
                {t("common.cancel")}
              </Button>
              <Button
                type="button"
                disabled={actionBusy || !createName.trim()}
                onClick={() =>
                  void runAction({
                    type: "create",
                    name: createName.trim(),
                    driver: createDriver,
                    subnet: createSubnet.trim() || undefined,
                    gateway: createGateway.trim() || undefined,
                    labels: parseLabels(createLabels),
                  })
                }
              >
                {t("operations.dockerNetworks.create")}
              </Button>
            </div>
          </div>
        </DialogShell>
      )}

      {confirm && (
        <DialogShell
          title={t("operations.confirmTitle")}
          onClose={() => !actionBusy && setConfirm(null)}
          closeDisabled={actionBusy}
        >
          <p className="text-sm text-[hsl(var(--secondary))]">
            {confirm.kind === "prune"
              ? t("operations.dockerNetworks.confirmPrune")
              : confirm.kind === "bulkRemove"
                ? t("operations.dockerNetworks.confirmBulkDelete", { count: confirm.ids.length })
                : t("operations.confirmDescription", {
                    action: t("operations.dockerNetworks.delete"),
                    target: confirm.label,
                  })}
          </p>
          {actionFailed && <div className="mt-3">{failureMessage}</div>}
          <div className="mt-5 flex justify-end gap-2">
            <Button type="button" variant="ghost" disabled={actionBusy} onClick={() => setConfirm(null)}>
              {t("common.cancel")}
            </Button>
            <Button
              type="button"
              variant={confirm.kind === "prune" ? "default" : "danger"}
              disabled={actionBusy}
              onClick={() => {
                if (confirm.kind === "prune") void runAction({ type: "prune" });
                else void runAction({ type: "remove", ids: confirm.ids });
              }}
            >
              {confirm.kind === "prune"
                ? t("operations.dockerNetworks.prune")
                : t("operations.dockerNetworks.delete")}
            </Button>
          </div>
        </DialogShell>
      )}
    </div>
  );
}
