import { zodResolver } from "@hookform/resolvers/zod";
import { useState } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { z } from "zod";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { useCatalogStore } from "../../stores/catalog-store";
import type { HostGroup } from "../../types/domain";

const schema = z.object({ name: z.string().trim().min(1).max(50) });
type Values = z.infer<typeof schema>;

export function GroupDialog({ group, onClose }: { group?: HostGroup; onClose: () => void }) {
  const { t } = useTranslation(); const [failure, setFailure] = useState(false); const createGroup = useCatalogStore((state) => state.createGroup); const updateGroup = useCatalogStore((state) => state.updateGroup);
  const { register, handleSubmit, formState: { errors, isSubmitting } } = useForm<Values>({ resolver: zodResolver(schema), defaultValues: { name: group?.name ?? "" } });
  const submit = handleSubmit(async ({ name }) => { setFailure(false); try { if (group) await updateGroup({ id: group.id, name, sortOrder: group.sortOrder, collapsed: group.collapsed }); else await createGroup({ name }); onClose(); } catch { setFailure(true); } });
  return <DialogShell title={t(group ? "group.editTitle" : "group.createTitle")} onClose={onClose}><form onSubmit={submit}><label className="block text-sm font-medium">{t("group.name")}<Input className="mt-1" autoFocus aria-invalid={Boolean(errors.name)} {...register("name")} /></label>{errors.name && <p className="mt-1 text-xs text-red-500">{t("validation.groupName")}</p>}{failure && <p className="mt-3 text-sm text-red-500">{t("errors.saveFailed")}</p>}<div className="mt-5 flex justify-end gap-2"><Button type="button" variant="ghost" onClick={onClose}>{t("common.cancel")}</Button><Button type="submit" disabled={isSubmitting}>{t("common.save")}</Button></div></form></DialogShell>;
}
