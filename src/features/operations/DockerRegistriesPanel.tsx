import { Pencil, Settings, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import { SelectControl } from "../../components/ui/select-control";
import type { DockerRegistry, DockerRegistryUpsertInput } from "../../types/infrastructure";
import { cn } from "../../lib/utils";

const PAGE_SIZE_OPTIONS = ["10", "20", "50", "100"] as const;

type FormState = {
  url: string;
  name: string;
  username: string;
  password: string;
  namespace: string;
  remarks: string;
};

const emptyForm = (): FormState => ({
  url: "",
  name: "",
  username: "",
  password: "",
  namespace: "",
  remarks: "",
});

type ConfirmKind = { kind: "remove"; ids: string[]; label: string } | { kind: "bulkRemove"; ids: string[] };

export function DockerRegistriesPanel({
  registries,
  busy = false,
  onUpsert,
  onDelete,
  onOpenSettings,
}: {
  registries: DockerRegistry[];
  busy?: boolean;
  onUpsert: (input: DockerRegistryUpsertInput) => Promise<void>;
  onDelete: (ids: string[]) => Promise<void>;
  onOpenSettings?: () => void;
}) {
  const { t } = useTranslation();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState<(typeof PAGE_SIZE_OPTIONS)[number]>("20");
  const [jumpPage, setJumpPage] = useState("1");
  const [bulkAction, setBulkAction] = useState<"none" | "remove">("none");
  const [editor, setEditor] = useState<{ mode: "create" } | { mode: "edit"; id: string } | null>(null);
  const [form, setForm] = useState<FormState>(emptyForm);
  const [confirm, setConfirm] = useState<ConfirmKind | null>(null);
  const [actionBusy, setActionBusy] = useState(false);
  const [actionFailed, setActionFailed] = useState(false);
  const [actionErrorDetail, setActionErrorDetail] = useState("");

  const registryList = registries ?? [];
  const pageSizeNumber = Number(pageSize);
  const totalPages = Math.max(1, Math.ceil(registryList.length / pageSizeNumber));
  const currentPage = Math.min(page, totalPages);
  const pageItems = registryList.slice((currentPage - 1) * pageSizeNumber, currentPage * pageSizeNumber);
  const allPageSelected = pageItems.length > 0 && pageItems.every((item) => selected.has(item.id));

  const pageNumbers = useMemo(() => {
    const windowSize = 5;
    const start = Math.max(1, Math.min(currentPage - 2, totalPages - windowSize + 1));
    const end = Math.min(totalPages, start + windowSize - 1);
    return Array.from({ length: end - start + 1 }, (_, index) => start + index);
  }, [currentPage, totalPages]);

  useEffect(() => {
    setPage(1);
  }, [pageSize, registryList.length]);

  useEffect(() => {
    setJumpPage(String(currentPage));
  }, [currentPage]);

  useEffect(() => {
    const valid = new Set(registryList.map((item) => item.id));
    setSelected((previous) => {
      const next = new Set([...previous].filter((id) => valid.has(id)));
      return next.size === previous.size ? previous : next;
    });
  }, [registryList]);

  const toggleAllPage = () => {
    setSelected((previous) => {
      const next = new Set(previous);
      if (allPageSelected) {
        for (const item of pageItems) next.delete(item.id);
      } else {
        for (const item of pageItems) next.add(item.id);
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

  const setField = <K extends keyof FormState>(key: K, value: FormState[K]) => {
    setForm((previous) => ({ ...previous, [key]: value }));
  };

  const openCreate = () => {
    setActionFailed(false);
    setActionErrorDetail("");
    setForm(emptyForm());
    setEditor({ mode: "create" });
  };

  const openEdit = (item: DockerRegistry) => {
    setActionFailed(false);
    setActionErrorDetail("");
    setForm({
      url: item.url,
      name: item.name,
      username: item.username,
      password: "",
      namespace: item.namespace,
      remarks: item.remarks,
    });
    setEditor({ mode: "edit", id: item.id });
  };

  const formValid =
    form.url.trim().length > 0 &&
    form.name.trim().length > 0 &&
    form.username.trim().length > 0 &&
    form.namespace.trim().length > 0 &&
    (editor?.mode === "edit" || form.password.trim().length > 0);

  const submitForm = async () => {
    if (!editor || !formValid) return;
    setActionBusy(true);
    setActionFailed(false);
    setActionErrorDetail("");
    try {
      const input: DockerRegistryUpsertInput = {
        id: editor.mode === "edit" ? editor.id : undefined,
        url: form.url.trim(),
        name: form.name.trim(),
        username: form.username.trim(),
        namespace: form.namespace.trim(),
        remarks: form.remarks.trim(),
      };
      if (form.password.trim()) input.password = form.password;
      await onUpsert(input);
      setEditor(null);
      setForm(emptyForm());
      toast.success(
        editor.mode === "create"
          ? t("operations.dockerRegistries.createSuccess")
          : t("operations.dockerRegistries.updateSuccess"),
      );
    } catch (error) {
      setActionFailed(true);
      setActionErrorDetail(error instanceof Error ? error.message : String(error));
    } finally {
      setActionBusy(false);
    }
  };

  const runDelete = async (ids: string[]) => {
    setActionBusy(true);
    setActionFailed(false);
    setActionErrorDetail("");
    try {
      await onDelete(ids);
      setConfirm(null);
      setSelected((previous) => {
        const next = new Set(previous);
        for (const id of ids) next.delete(id);
        return next;
      });
      toast.success(
        ids.length > 1
          ? t("operations.dockerRegistries.deleteSuccessBulk", { count: ids.length })
          : t("operations.dockerRegistries.deleteSuccess"),
      );
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
        <Button type="button" size="sm" disabled={busy || actionBusy} onClick={openCreate}>
          {t("operations.dockerRegistries.create")}
        </Button>
        {onOpenSettings ? (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="ml-auto h-8 w-8"
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
                  aria-label={t("operations.dockerRegistries.selectAll")}
                />
              </th>
              <th className="px-3 py-2">{t("operations.dockerRegistries.url")}</th>
              <th className="px-3 py-2">{t("operations.dockerRegistries.username")}</th>
              <th className="px-3 py-2">{t("operations.dockerRegistries.name")}</th>
              <th className="px-3 py-2">{t("operations.dockerRegistries.namespace")}</th>
              <th className="w-24 px-3 py-2">{t("operations.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {pageItems.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-3 py-8 text-center text-[hsl(var(--muted))]">
                  {t("operations.dockerRegistries.empty")}
                </td>
              </tr>
            ) : (
              pageItems.map((item) => (
                <tr key={item.id} className="border-t hover:bg-[hsl(var(--elevated)/.45)]">
                  <td className="px-3 py-2">
                    <input
                      type="checkbox"
                      checked={selected.has(item.id)}
                      onChange={() => toggleOne(item.id)}
                      aria-label={t("operations.dockerRegistries.selectRow", { name: item.name })}
                    />
                  </td>
                  <td className="max-w-xs truncate px-3 py-2 font-mono text-xs" title={item.url}>
                    {item.url}
                  </td>
                  <td className="max-w-[10rem] truncate px-3 py-2" title={item.username}>
                    {item.username}
                  </td>
                  <td className="max-w-[12rem] truncate px-3 py-2" title={item.name}>
                    {item.name}
                  </td>
                  <td className="max-w-[10rem] truncate px-3 py-2 font-mono text-xs" title={item.namespace}>
                    {item.namespace}
                  </td>
                  <td className="px-3 py-2">
                    <div className="flex items-center gap-0.5">
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        title={t("operations.dockerRegistries.edit")}
                        aria-label={t("operations.dockerRegistries.edit")}
                        disabled={busy || actionBusy}
                        onClick={() => openEdit(item)}
                      >
                        <Pencil size={13} />
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 text-red-500 hover:text-red-500"
                        title={t("operations.dockerRegistries.delete")}
                        aria-label={t("operations.dockerRegistries.delete")}
                        disabled={busy || actionBusy}
                        onClick={() => {
                          setActionFailed(false);
                          setConfirm({ kind: "remove", ids: [item.id], label: item.name });
                        }}
                      >
                        <Trash2 size={13} />
                      </Button>
                    </div>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <SelectControl
            value={bulkAction}
            onValueChange={setBulkAction}
            label={t("operations.dockerRegistries.bulkAction")}
            className="w-44"
            options={[
              { value: "none", label: t("operations.dockerRegistries.bulkPlaceholder") },
              { value: "remove", label: t("operations.dockerRegistries.bulkDelete") },
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
            {t("operations.dockerRegistries.bulkRun")}
          </Button>
        </div>

        <div className="ml-auto flex flex-wrap items-center gap-2 text-xs text-[hsl(var(--muted))]">
          <div className="flex items-center gap-1">
            {pageNumbers.map((number) => (
              <Button
                key={number}
                type="button"
                size="sm"
                variant={number === currentPage ? "secondary" : "ghost"}
                className={cn("h-8 min-w-8 px-2", number === currentPage && "pointer-events-none")}
                onClick={() => setPage(number)}
              >
                {number}
              </Button>
            ))}
          </div>
          <SelectControl
            value={pageSize}
            onValueChange={setPageSize}
            label={t("operations.dockerRegistries.pageSize")}
            className="w-28"
            options={PAGE_SIZE_OPTIONS.map((value) => ({
              value,
              label: t("operations.dockerRegistries.pageSizeOption", { count: Number(value) }),
            }))}
          />
          <span>{t("operations.dockerRegistries.total", { count: registryList.length })}</span>
          <span className="flex items-center gap-1">
            {t("operations.dockerRegistries.goto")}
            <Input
              value={jumpPage}
              onChange={(event) => setJumpPage(event.target.value)}
              onKeyDown={(event) => {
                if (event.key !== "Enter") return;
                const next = Number(jumpPage);
                if (Number.isFinite(next)) setPage(Math.min(totalPages, Math.max(1, Math.trunc(next))));
              }}
              className="h-8 w-14 px-2"
              aria-label={t("operations.dockerRegistries.goto")}
            />
            {t("operations.dockerRegistries.page")}
          </span>
        </div>
      </div>

      {editor && (
        <DialogShell
          title={
            editor.mode === "create"
              ? t("operations.dockerRegistries.createTitle")
              : t("operations.dockerRegistries.editTitle")
          }
          onClose={() => !actionBusy && setEditor(null)}
          closeDisabled={actionBusy}
          size="form"
        >
          <div className="space-y-3">
            <div className="space-y-1.5">
              <Label htmlFor="docker-registry-url">
                <span className="text-red-500">*</span> {t("operations.dockerRegistries.url")}
              </Label>
              <Input
                id="docker-registry-url"
                value={form.url}
                placeholder={t("operations.dockerRegistries.urlPlaceholder")}
                onChange={(event) => setField("url", event.target.value)}
                disabled={actionBusy}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="docker-registry-name">
                <span className="text-red-500">*</span> {t("operations.dockerRegistries.name")}
              </Label>
              <Input
                id="docker-registry-name"
                value={form.name}
                placeholder={t("operations.dockerRegistries.namePlaceholder")}
                onChange={(event) => setField("name", event.target.value)}
                disabled={actionBusy}
              />
            </div>
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label htmlFor="docker-registry-username">
                  <span className="text-red-500">*</span> {t("operations.dockerRegistries.username")}
                </Label>
                <Input
                  id="docker-registry-username"
                  value={form.username}
                  placeholder={t("operations.dockerRegistries.usernamePlaceholder")}
                  onChange={(event) => setField("username", event.target.value)}
                  disabled={actionBusy}
                  autoComplete="off"
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="docker-registry-password">
                  {editor.mode === "create" ? <span className="text-red-500">*</span> : null}{" "}
                  {t("operations.dockerRegistries.password")}
                </Label>
                <Input
                  id="docker-registry-password"
                  type="password"
                  value={form.password}
                  placeholder={
                    editor.mode === "edit"
                      ? t("operations.dockerRegistries.passwordKeepPlaceholder")
                      : t("operations.dockerRegistries.passwordPlaceholder")
                  }
                  onChange={(event) => setField("password", event.target.value)}
                  disabled={actionBusy}
                  autoComplete="new-password"
                />
              </div>
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="docker-registry-namespace">
                <span className="text-red-500">*</span> {t("operations.dockerRegistries.namespace")}
              </Label>
              <Input
                id="docker-registry-namespace"
                value={form.namespace}
                placeholder={t("operations.dockerRegistries.namespacePlaceholder")}
                onChange={(event) => setField("namespace", event.target.value)}
                disabled={actionBusy}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="docker-registry-remarks">{t("operations.dockerRegistries.remarks")}</Label>
              <Input
                id="docker-registry-remarks"
                value={form.remarks}
                placeholder={t("operations.dockerRegistries.remarksPlaceholder")}
                onChange={(event) => setField("remarks", event.target.value)}
                disabled={actionBusy}
              />
            </div>
            {actionFailed && failureMessage}
            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" disabled={actionBusy} onClick={() => setEditor(null)}>
                {t("common.cancel")}
              </Button>
              <Button type="button" disabled={actionBusy || !formValid} onClick={() => void submitForm()}>
                {editor.mode === "create"
                  ? t("operations.dockerRegistries.create")
                  : t("operations.dockerRegistries.save")}
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
            {confirm.kind === "bulkRemove"
              ? t("operations.dockerRegistries.confirmBulkDelete", { count: confirm.ids.length })
              : t("operations.dockerRegistries.confirmDelete", { name: confirm.label })}
          </p>
          {actionFailed && <div className="mt-3">{failureMessage}</div>}
          <div className="mt-5 flex justify-end gap-2">
            <Button type="button" variant="ghost" disabled={actionBusy} onClick={() => setConfirm(null)}>
              {t("common.cancel")}
            </Button>
            <Button type="button" variant="danger" disabled={actionBusy} onClick={() => void runDelete(confirm.ids)}>
              {t("operations.dockerRegistries.delete")}
            </Button>
          </div>
        </DialogShell>
      )}
    </div>
  );
}
