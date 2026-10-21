import { zodResolver } from "@hookform/resolvers/zod";
import { useState } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Input } from "../../components/ui/input";
import { SearchableCombobox } from "../../components/ui/searchable-combobox";
import { appErrorCode } from "../../lib/app-error";
import { useCatalogStore } from "../../stores/catalog-store";
import type { ServerProfile } from "../../types/domain";
import type { SaveTunnelRequest, TunnelRule } from "../../types/tunnels";
import { profileComboboxGroups, tunnelSchema, type TunnelFormValues } from "./tunnel-form";

export function TunnelForm({ rule, initialProfileId, profiles, onSave, onClose }: {
  rule?: TunnelRule;
  initialProfileId?: string;
  profiles: ServerProfile[];
  onSave: (request: SaveTunnelRequest, start: boolean) => Promise<void>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const groups = useCatalogStore((store) => store.groups);
  const [failure, setFailure] = useState<string | null>(null);
  const profileGroups = profileComboboxGroups(groups, profiles, t("sidebar.ungrouped"));
  const { register, watch, setValue, handleSubmit, formState: { errors, isSubmitting } } = useForm<TunnelFormValues>({
    resolver: zodResolver(tunnelSchema),
    defaultValues: rule ?? { name: "", profileId: initialProfileId ?? profiles[0]?.id ?? "", targetHost: "" },
  });
  const values = watch();
  const profile = profiles.find((item) => item.id === values.profileId);
  const submit = (start: boolean) => handleSubmit(async (value) => {
    setFailure(null);
    try { await onSave({ ...value, id: rule?.id }, start); }
    catch (error) { setFailure(appErrorCode(error)); }
  });
  return <DialogShell title={t(rule ? "tunnels.edit" : "tunnels.create")} onClose={() => { if (!isSubmitting) onClose(); }}>
    <form className="tunnel-form" onSubmit={(event) => void submit(false)(event)}>
      <label htmlFor="tunnel-name">{t("tunnels.name")}<Input id="tunnel-name" autoFocus {...register("name")} aria-invalid={!!errors.name} /></label>
      <label htmlFor="tunnel-profile">{t("tunnels.via")}<SearchableCombobox id="tunnel-profile" label={t("tunnels.via")} value={values.profileId} groups={profileGroups} placeholder={t("tunnels.selectServer")} searchPlaceholder={t("tunnels.serverSearchPlaceholder")} emptyMessage={t("tunnels.noMatchingServers")} disabled={isSubmitting} invalid={!!errors.profileId} onValueChange={(value) => setValue("profileId", value, { shouldValidate: true })} /></label>
      <div className="tunnel-form-pair">
        <label htmlFor="tunnel-host">{t("tunnels.targetHost")}<Input id="tunnel-host" {...register("targetHost")} aria-invalid={!!errors.targetHost} aria-describedby="tunnel-target-hint" /></label>
        <label htmlFor="tunnel-target-port">{t("tunnels.targetPort")}<Input id="tunnel-target-port" type="number" min={1} max={65535} {...register("targetPort", { valueAsNumber: true })} aria-invalid={!!errors.targetPort} /></label>
      </div>
      <p id="tunnel-target-hint" className="tunnel-hint">{t("tunnels.targetHint")}</p>
      <div className="tunnel-form-pair">
        <label htmlFor="tunnel-bind">{t("tunnels.access")}<Input id="tunnel-bind" readOnly value={t("tunnels.loopback")} /></label>
        <label htmlFor="tunnel-local-port">{t("tunnels.localPort")}<Input id="tunnel-local-port" type="number" min={1} max={65535} {...register("localPort", { valueAsNumber: true })} aria-invalid={!!errors.localPort} /></label>
      </div>
      <p className="tunnel-route-preview">{t("tunnels.preview", { server: profile?.name ?? t("tunnels.missingProfile"), target: values.targetHost || "…", port: Number.isFinite(values.targetPort) ? values.targetPort : "…", localPort: Number.isFinite(values.localPort) ? values.localPort : "…" })}</p>
      <p className="tunnel-hint">{t("tunnels.securityHint")}</p>
      {(Object.keys(errors).length > 0 || !profile) && <p role="alert" className="tunnel-error">{t("tunnels.invalidForm")}</p>}
      {failure && <p role="alert" className="tunnel-error">{t(`errors.${failure}`, { defaultValue: t("errors.UNKNOWN") })}</p>}
      <div className="tunnel-form-actions">
        <Button type="button" variant="ghost" disabled={isSubmitting} onClick={onClose}>{t("common.cancel")}</Button>
        <Button type="submit" variant="secondary" disabled={isSubmitting || !profile}>{t("common.save")}</Button>
        <Button type="button" disabled={isSubmitting || !profile} onClick={() => void submit(true)()}>{t("tunnels.saveStart")}</Button>
      </div>
    </form>
  </DialogShell>;
}
