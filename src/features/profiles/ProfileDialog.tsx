import { zodResolver } from "@hookform/resolvers/zod";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertCircle, Check, FolderOpen, KeyRound, Loader2, Network, Server, ShieldCheck } from "lucide-react";
import { type ReactNode, useRef, useState } from "react";
import { Controller, useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { z } from "zod";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import { SearchableCombobox } from "../../components/ui/searchable-combobox";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";
import { forgetPrivateKey, importPrivateKey } from "../../lib/tauri/ssh";
import { useCatalogStore } from "../../stores/catalog-store";
import type { ServerProfile } from "../../types/domain";

const schema = z.object({
  name: z.string().trim().min(1).max(100),
  host: z.string().trim().min(1).max(255).refine((value) => !/\s/.test(value)),
  port: z.number().int().min(1).max(65535),
  username: z.string().trim().min(1).max(255),
  groupId: z.string(),
  authMethod: z.enum(["password", "privateKey"]),
  routeType: z.enum(["direct", "jumpHost"]),
  jumpProfileId: z.string(),
  keySourceType: z.enum(["file", "vault"]),
  keyPath: z.string(),
  keyId: z.string(),
}).superRefine((values, context) => {
  if (values.routeType === "jumpHost" && !values.jumpProfileId) context.addIssue({ code: "custom", path: ["jumpProfileId"], message: "required" });
  if (values.authMethod !== "privateKey") return;
  if (values.keySourceType === "file" && !values.keyPath.trim()) context.addIssue({ code: "custom", path: ["keyPath"], message: "required" });
  if (values.keySourceType === "vault" && !values.keyId) context.addIssue({ code: "custom", path: ["keyId"], message: "required" });
});

type Values = z.infer<typeof schema>;
type FieldProps = { id: string; label: string; error?: string; children: ReactNode; hint?: string };

function FormField({ id, label, error, children, hint }: FieldProps) {
  return <div className="space-y-1.5">
    <Label htmlFor={id}>{label}</Label>
    {children}
    {error
      ? <p id={`${id}-error`} role="alert" className="flex items-center gap-1.5 text-xs text-red-500"><AlertCircle size={12} aria-hidden="true" />{error}</p>
      : hint && <p id={`${id}-hint`} className="text-xs leading-relaxed text-[hsl(var(--muted))]">{hint}</p>}
  </div>;
}

function FormSection({ icon, title, description, children }: { icon: ReactNode; title: string; description: string; children: ReactNode }) {
  return <section className="space-y-4" aria-label={title}>
    <div className="flex items-start gap-3">
      <div className="mt-0.5 grid h-8 w-8 shrink-0 place-items-center rounded-md bg-[hsl(var(--primary)/.1)] text-[hsl(var(--primary))]" aria-hidden="true">{icon}</div>
      <div className="min-w-0">
        <h3 className="text-sm font-semibold">{title}</h3>
        <p className="mt-0.5 text-xs leading-relaxed text-[hsl(var(--muted))]">{description}</p>
      </div>
    </div>
    {children}
  </section>;
}

export function ProfileDialog({ profile, initialGroupId = null, onClose }: { profile?: ServerProfile; initialGroupId?: string | null; onClose: () => void }) {
  const { t } = useTranslation();
  const groups = useCatalogStore((state) => state.groups);
  const profiles = useCatalogStore((state) => state.profiles);
  const createProfile = useCatalogStore((state) => state.createProfile);
  const updateProfile = useCatalogStore((state) => state.updateProfile);
  const [failure, setFailure] = useState(false);
  const [keyLabel, setKeyLabel] = useState(profile?.keySource?.type === "vault" ? t("profile.vaultKeyStored") : "");
  const [importing, setImporting] = useState(false);
  const importedKeyId = useRef<string | null>(null);
  const committed = useRef(false);
  const { control, register, handleSubmit, setValue, watch, formState: { errors, isSubmitting } } = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: {
      name: profile?.name ?? "",
      host: profile?.host ?? "",
      port: profile?.port ?? 22,
      username: profile?.username ?? "",
      groupId: profile?.groupId ?? initialGroupId ?? "",
      authMethod: profile?.authMethod ?? "password",
      routeType: profile?.connectionRoute.type ?? "direct",
      jumpProfileId: profile?.connectionRoute.type === "jumpHost" ? profile.connectionRoute.profileId : "",
      keySourceType: profile?.keySource?.type ?? (matchMedia("(pointer: coarse)").matches ? "vault" : "file"),
      keyPath: profile?.keySource?.type === "file" ? profile.keySource.path : "",
      keyId: profile?.keySource?.type === "vault" ? profile.keySource.keyId : "",
    },
  });
  const authMethod = watch("authMethod");
  const keySourceType = watch("keySourceType");
  const routeType = watch("routeType");
  const jumpCandidates = profiles.filter((candidate) => candidate.id !== profile?.id && candidate.connectionRoute.type === "direct");
  const groupOptions = [
    { value: "", label: t("sidebar.ungrouped") },
    ...groups.map((group) => ({ value: group.id, label: group.name })),
  ];
  const jumpHostOptions = jumpCandidates.map((candidate) => ({
    value: candidate.id,
    label: `${candidate.name} · ${candidate.host}:${candidate.port}`,
    keywords: `${candidate.name} ${candidate.host} ${candidate.port} ${candidate.username}`,
  }));
  const busy = isSubmitting || importing;

  const discardImported = async () => {
    const keyId = importedKeyId.current;
    importedKeyId.current = null;
    if (keyId && !committed.current) await forgetPrivateKey(keyId).catch(() => undefined);
  };
  const close = () => {
    if (busy) return;
    void discardImported();
    onClose();
  };
  const browse = async () => {
    const selected = await open({ multiple: false, directory: false, title: t("profile.selectPrivateKey") });
    if (typeof selected === "string") setValue("keyPath", selected, { shouldDirty: true, shouldValidate: true });
  };
  const importKey = async () => {
    setFailure(false);
    setImporting(true);
    try {
      const selected = await importPrivateKey();
      if (!selected) return;
      await discardImported();
      importedKeyId.current = selected.keyId;
      setValue("keyId", selected.keyId, { shouldDirty: true, shouldValidate: true });
      setKeyLabel(selected.name);
    } catch {
      setFailure(true);
    } finally {
      setImporting(false);
    }
  };
  const submit = handleSubmit(async (values) => {
    setFailure(false);
    const keySource = values.authMethod !== "privateKey"
      ? undefined
      : values.keySourceType === "file"
        ? { type: "file" as const, path: values.keyPath.trim() }
        : { type: "vault" as const, keyId: values.keyId };
    const connectionRoute = values.routeType === "jumpHost"
      ? { type: "jumpHost" as const, profileId: values.jumpProfileId }
      : { type: "direct" as const };
    const request = {
      name: values.name.trim(),
      host: values.host.trim(),
      port: values.port,
      username: values.username.trim(),
      groupId: values.groupId || null,
      authMethod: values.authMethod,
      keySource,
      connectionRoute,
    };
    try {
      if (profile) await updateProfile({ id: profile.id, sortOrder: profile.sortOrder, ...request });
      else await createProfile(request);
      committed.current = true;
      onClose();
    } catch {
      setFailure(true);
    }
  });

  return <DialogShell title={t(profile ? "profile.editTitle" : "profile.createTitle")} size="form" closeDisabled={busy} onClose={close}>
    <form className="-mx-5 -mb-5" noValidate onSubmit={submit}>
      <div className="space-y-6 px-5 pb-6">
        <FormSection icon={<Server size={17} />} title={t("profile.detailsSection")} description={t("profile.detailsSectionHint")}>
          <div className="space-y-4">
            <FormField id="profile-name" label={t("profile.name")} error={errors.name ? t("validation.profileName") : undefined}>
              <Input id="profile-name" autoFocus={!profile} autoComplete="off" placeholder={t("profile.namePlaceholder")} aria-invalid={Boolean(errors.name)} aria-describedby={errors.name ? "profile-name-error" : undefined} disabled={busy} {...register("name")} />
            </FormField>
            <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_112px]">
              <FormField id="profile-host" label={t("profile.host")} error={errors.host ? t("validation.profileHost") : undefined}>
                <Input id="profile-host" autoCapitalize="none" autoCorrect="off" spellCheck={false} placeholder={t("profile.hostPlaceholder")} aria-invalid={Boolean(errors.host)} aria-describedby={errors.host ? "profile-host-error" : undefined} disabled={busy} {...register("host")} />
              </FormField>
              <FormField id="profile-port" label={t("profile.port")} error={errors.port ? t("validation.profilePort") : undefined}>
                <Input id="profile-port" type="number" inputMode="numeric" min={1} max={65535} aria-invalid={Boolean(errors.port)} aria-describedby={errors.port ? "profile-port-error" : undefined} disabled={busy} {...register("port", { valueAsNumber: true })} />
              </FormField>
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              <FormField id="profile-username" label={t("profile.username")} error={errors.username ? t("validation.profileUsername") : undefined}>
                <Input id="profile-username" autoCapitalize="none" autoCorrect="off" spellCheck={false} autoComplete="username" placeholder={t("profile.usernamePlaceholder")} aria-invalid={Boolean(errors.username)} aria-describedby={errors.username ? "profile-username-error" : undefined} disabled={busy} {...register("username")} />
              </FormField>
              <FormField id="profile-group" label={t("profile.group")}>
                <Controller control={control} name="groupId" render={({ field }) => <SearchableCombobox
                  id="profile-group"
                  label={t("profile.group")}
                  value={field.value}
                  options={groupOptions}
                  placeholder={t("sidebar.ungrouped")}
                  searchPlaceholder={t("profile.groupSearchPlaceholder")}
                  emptyMessage={t("profile.noMatchingGroups")}
                  disabled={busy}
                  onValueChange={field.onChange}
                />} />
              </FormField>
            </div>
          </div>
        </FormSection>

        <div className="border-t border-[hsl(var(--border-soft))]" />

        <FormSection icon={<ShieldCheck size={17} />} title={t("profile.accessSection")} description={t("profile.accessSectionHint")}>
          <div className="grid gap-4 sm:grid-cols-2">
            <FormField id="profile-route" label={t("profile.connectionRoute")}>
              <Controller control={control} name="routeType" render={({ field }) => <Select value={field.value} disabled={busy} onValueChange={field.onChange}>
                <SelectTrigger id="profile-route"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="direct">{t("profile.routeDirect")}</SelectItem>
                  <SelectItem value="jumpHost">{t("profile.routeJumpHost")}</SelectItem>
                </SelectContent>
              </Select>} />
            </FormField>
            <FormField id="profile-auth" label={t("profile.authMethod")}>
              <Controller control={control} name="authMethod" render={({ field }) => <Select value={field.value} disabled={busy} onValueChange={field.onChange}>
                <SelectTrigger id="profile-auth"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="password">{t("profile.password")}</SelectItem>
                  <SelectItem value="privateKey">{t("profile.privateKey")}</SelectItem>
                </SelectContent>
              </Select>} />
            </FormField>
          </div>

          {routeType === "jumpHost" && <div className="rounded-lg border bg-[hsl(var(--elevated)/.45)] p-4">
            <FormField id="profile-jump-host" label={t("profile.jumpHost")} error={errors.jumpProfileId ? t("validation.profileJumpHost") : undefined} hint={t(jumpCandidates.length ? "profile.jumpHostHint" : "profile.noJumpHosts")}>
              <Controller control={control} name="jumpProfileId" render={({ field }) => <SearchableCombobox
                id="profile-jump-host"
                label={t("profile.jumpHost")}
                value={field.value}
                options={jumpHostOptions}
                placeholder={t("profile.selectJumpHost")}
                searchPlaceholder={t("profile.jumpHostSearchPlaceholder")}
                emptyMessage={t("profile.noMatchingJumpHosts")}
                disabled={busy || jumpCandidates.length === 0}
                invalid={Boolean(errors.jumpProfileId)}
                describedBy={errors.jumpProfileId ? "profile-jump-host-error" : "profile-jump-host-hint"}
                onValueChange={field.onChange}
              />} />
            </FormField>
          </div>}

          {authMethod === "privateKey" && <div className="space-y-4 rounded-lg border bg-[hsl(var(--elevated)/.45)] p-4">
            <FormField id="profile-key-storage" label={t("profile.keyStorage")}>
              <Controller control={control} name="keySourceType" render={({ field }) => <Select value={field.value} disabled={busy} onValueChange={field.onChange}>
                <SelectTrigger id="profile-key-storage"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="file">{t("profile.keyFile")}</SelectItem>
                  <SelectItem value="vault">{t("profile.keyVault")}</SelectItem>
                </SelectContent>
              </Select>} />
            </FormField>
            {keySourceType === "file"
              ? <FormField id="profile-key-path" label={t("profile.privateKey")} error={errors.keyPath ? t("validation.profilePrivateKey") : undefined} hint={t("profile.privateKeyHint")}>
                <div className="flex gap-2">
                  <Input id="profile-key-path" className="min-w-0 font-mono" readOnly placeholder={t("profile.keyPathPlaceholder")} aria-invalid={Boolean(errors.keyPath)} aria-describedby={errors.keyPath ? "profile-key-path-error" : "profile-key-path-hint"} disabled={busy} {...register("keyPath")} />
                  <Button type="button" variant="secondary" disabled={busy} onClick={() => void browse()}><FolderOpen size={16} aria-hidden="true" /><span className="hidden sm:inline">{t("profile.browse")}</span></Button>
                </div>
              </FormField>
              : <FormField id="profile-key-vault" label={t("profile.privateKey")} error={errors.keyId ? t("validation.profilePrivateKey") : undefined} hint={t("profile.vaultKeyHint")}>
                <input type="hidden" {...register("keyId")} />
                <Button id="profile-key-vault" type="button" className="max-w-full justify-start" variant="secondary" disabled={busy} aria-describedby={errors.keyId ? "profile-key-vault-error" : "profile-key-vault-hint"} onClick={() => void importKey()}>
                  {importing ? <Loader2 size={16} className="animate-spin" aria-hidden="true" /> : keyLabel ? <Check size={16} className="text-emerald-500" aria-hidden="true" /> : <KeyRound size={16} aria-hidden="true" />}
                  <span className="truncate">{keyLabel || t("profile.importToVault")}</span>
                </Button>
              </FormField>}
          </div>}
        </FormSection>

        {failure && <div role="alert" className="flex items-start gap-2 rounded-md border border-red-500/30 bg-red-500/5 p-3 text-sm text-red-500"><AlertCircle size={16} className="mt-0.5 shrink-0" aria-hidden="true" /><span>{t("errors.saveFailed")}</span></div>}
      </div>

      <div className="sticky bottom-0 flex items-center justify-between gap-3 border-t bg-[hsl(var(--surface)/.96)] px-5 py-4 pb-[calc(1rem+env(safe-area-inset-bottom))] backdrop-blur-sm">
        <p className="hidden items-center gap-1.5 text-xs text-[hsl(var(--muted))] sm:flex"><Network size={13} aria-hidden="true" />{t("profile.localOnlyHint")}</p>
        <div className="ml-auto flex gap-2">
          <Button type="button" variant="ghost" disabled={busy} onClick={close}>{t("common.cancel")}</Button>
          <Button type="submit" disabled={busy}>{isSubmitting && <Loader2 size={16} className="animate-spin" aria-hidden="true" />}{isSubmitting ? t("common.saving") : t("common.save")}</Button>
        </div>
      </div>
    </form>
  </DialogShell>;
}
