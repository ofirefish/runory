import { zodResolver } from "@hookform/resolvers/zod";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, KeyRound } from "lucide-react";
import { useRef, useState } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { z } from "zod";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Input } from "../../components/ui/input";
import { forgetPrivateKey, importPrivateKey } from "../../lib/tauri/ssh";
import { useCatalogStore } from "../../stores/catalog-store";
import type { ServerProfile } from "../../types/domain";

const schema = z.object({
  name: z.string().trim().min(1).max(100),
  host: z.string().trim().min(1).max(255).refine((value) => !/\s/.test(value)),
  port: z.number().int().min(1).max(65535), username: z.string().trim().min(1).max(255),
  groupId: z.string(), authMethod: z.enum(["password", "privateKey"]),
  keySourceType: z.enum(["file", "vault"]), keyPath: z.string(), keyId: z.string(),
}).superRefine((values, context) => {
  if (values.authMethod !== "privateKey") return;
  if (values.keySourceType === "file" && !values.keyPath.trim()) context.addIssue({ code: "custom", path: ["keyPath"], message: "required" });
  if (values.keySourceType === "vault" && !values.keyId) context.addIssue({ code: "custom", path: ["keyId"], message: "required" });
});
type Values = z.infer<typeof schema>;

export function ProfileDialog({ profile, initialGroupId = null, onClose }: { profile?: ServerProfile; initialGroupId?: string | null; onClose: () => void }) {
  const { t } = useTranslation();
  const groups = useCatalogStore((state) => state.groups);
  const createProfile = useCatalogStore((state) => state.createProfile);
  const updateProfile = useCatalogStore((state) => state.updateProfile);
  const [failure, setFailure] = useState(false);
  const [keyLabel, setKeyLabel] = useState(profile?.keySource?.type === "vault" ? t("profile.vaultKeyStored") : "");
  const [importing, setImporting] = useState(false);
  const importedKeyId = useRef<string | null>(null);
  const committed = useRef(false);
  const { register, handleSubmit, setValue, watch, formState: { errors, isSubmitting } } = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: {
      name: profile?.name ?? "", host: profile?.host ?? "", port: profile?.port ?? 22,
      username: profile?.username ?? "", groupId: profile?.groupId ?? initialGroupId ?? "",
      authMethod: profile?.authMethod ?? "password",
      keySourceType: profile?.keySource?.type ?? (matchMedia("(pointer: coarse)").matches ? "vault" : "file"),
      keyPath: profile?.keySource?.type === "file" ? profile.keySource.path : "",
      keyId: profile?.keySource?.type === "vault" ? profile.keySource.keyId : "",
    },
  });
  const authMethod = watch("authMethod"); const keySourceType = watch("keySourceType");
  const discardImported = async () => { const keyId = importedKeyId.current; importedKeyId.current = null; if (keyId && !committed.current) await forgetPrivateKey(keyId).catch(() => undefined); };
  const close = () => { void discardImported(); onClose(); };
  const browse = async () => { const selected = await open({ multiple: false, directory: false, title: t("profile.selectPrivateKey") }); if (typeof selected === "string") setValue("keyPath", selected, { shouldDirty: true, shouldValidate: true }); };
  const importKey = async () => {
    setFailure(false); setImporting(true);
    try { const selected = await importPrivateKey(); if (!selected) return; await discardImported(); importedKeyId.current = selected.keyId; setValue("keyId", selected.keyId, { shouldDirty: true, shouldValidate: true }); setKeyLabel(selected.name); }
    catch { setFailure(true); } finally { setImporting(false); }
  };
  const submit = handleSubmit(async (values) => {
    setFailure(false);
    const keySource = values.authMethod !== "privateKey" ? undefined : values.keySourceType === "file" ? { type: "file" as const, path: values.keyPath.trim() } : { type: "vault" as const, keyId: values.keyId };
    const request = { name: values.name, host: values.host, port: values.port, username: values.username, groupId: values.groupId || null, authMethod: values.authMethod, keySource };
    try { if (profile) await updateProfile({ id: profile.id, sortOrder: profile.sortOrder, ...request }); else await createProfile(request); committed.current = true; onClose(); } catch { setFailure(true); }
  });
  const fields = ["name", "host", "port", "username"] as const;
  return <DialogShell title={t(profile ? "profile.editTitle" : "profile.createTitle")} onClose={close}><form className="space-y-4" onSubmit={submit}>
    {fields.map((field) => <label key={field} className="block text-sm font-medium">{t(`profile.${field}`)}<Input className="mt-1" type={field === "port" ? "number" : "text"} aria-invalid={Boolean(errors[field])} {...register(field, field === "port" ? { valueAsNumber: true } : undefined)} /></label>)}
    <label className="block text-sm font-medium">{t("profile.group")}<select className="mt-1 h-10 w-full rounded-md border bg-[hsl(var(--surface))] px-3 text-sm" {...register("groupId")}><option value="">{t("sidebar.ungrouped")}</option>{groups.map((group) => <option key={group.id} value={group.id}>{group.name}</option>)}</select></label>
    <label className="block text-sm font-medium">{t("profile.authMethod")}<select className="mt-1 h-10 w-full rounded-md border bg-[hsl(var(--surface))] px-3 text-sm" {...register("authMethod")}><option value="password">{t("profile.password")}</option><option value="privateKey">{t("profile.privateKey")}</option></select></label>
    {authMethod === "privateKey" && <div className="space-y-3 rounded-md border p-3"><label className="block text-sm font-medium">{t("profile.keyStorage")}<select className="mt-1 h-10 w-full rounded-md border bg-[hsl(var(--surface))] px-3 text-sm" {...register("keySourceType")}><option value="file">{t("profile.keyFile")}</option><option value="vault">{t("profile.keyVault")}</option></select></label>
      {keySourceType === "file" ? <label className="block text-sm font-medium">{t("profile.privateKey")}<div className="mt-1 flex gap-2"><Input className="min-w-0 font-mono" readOnly aria-invalid={Boolean(errors.keyPath)} {...register("keyPath")} /><Button type="button" variant="secondary" onClick={() => void browse()}><FolderOpen size={16} /><span className="hidden sm:inline">{t("profile.browse")}</span></Button></div><span className="mt-1 block text-xs text-[hsl(var(--muted))]">{t("profile.privateKeyHint")}</span></label>
      : <div><input type="hidden" {...register("keyId")} /><Button type="button" variant="secondary" disabled={importing} onClick={() => void importKey()}><KeyRound size={16} />{keyLabel || t("profile.importToVault")}</Button><p className="mt-2 text-xs text-[hsl(var(--muted))]">{t("profile.vaultKeyHint")}</p></div>}</div>}
    {Object.keys(errors).length > 0 && <p className="text-xs text-red-500">{t("validation.profile")}</p>}{failure && <p className="text-sm text-red-500">{t("errors.saveFailed")}</p>}
    <div className="flex justify-end gap-2"><Button type="button" variant="ghost" onClick={close}>{t("common.cancel")}</Button><Button type="submit" disabled={isSubmitting}>{t("common.save")}</Button></div>
  </form></DialogShell>;
}
