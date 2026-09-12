import { zodResolver } from "@hookform/resolvers/zod";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertCircle, Check, FolderOpen, KeyRound, Loader2 } from "lucide-react";
import { type ReactNode, useRef, useState } from "react";
import { Controller, useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { z } from "zod";
import { Button } from "../../components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../components/ui/dialog";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import { SearchableCombobox } from "../../components/ui/searchable-combobox";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../../components/ui/tabs";
import { forgetPrivateKey, importPrivateKey } from "../../lib/tauri/ssh";
import { useCatalogStore } from "../../stores/catalog-store";
import { useSettingsStore } from "../../stores/settings-store";
import type { ServerProfile } from "../../types/domain";
import type { SettingsSection } from "../settings/SettingsPanel";

const schema = z.object({
  name: z.string().trim().min(1).max(100),
  host: z.string().trim().max(255).refine((value) => !value || !/\s/.test(value)),
  port: z.number().int().min(1).max(65535),
  username: z.string().trim().max(255),
  groupId: z.string(),
  authMethod: z.enum(["password", "privateKey"]),
  routeType: z.enum(["direct", "jumpHost", "bastion"]),
  jumpProfileId: z.string(),
  bastionProvider: z.string(),
  bastionApiBaseUrl: z.string(),
  bastionOrgId: z.string(),
  bastionAssetId: z.string(),
  bastionAccountId: z.string(),
  teleportClusterName: z.string(),
  teleportInsecure: z.boolean(),
  keySourceType: z.enum(["file", "vault"]),
  keyPath: z.string(),
  keyId: z.string(),
}).superRefine((values, context) => {
  const isJumpServer = values.routeType === "bastion" && values.bastionProvider === "jumpserver";
  const isBoundary = values.routeType === "bastion" && values.bastionProvider === "boundary";
  const isTeleport = values.routeType === "bastion" && values.bastionProvider === "teleport";
  if (!isJumpServer && !isBoundary && !values.host.trim()) {
    context.addIssue({ code: "custom", path: ["host"], message: "required" });
  }
  if (!isJumpServer && !isBoundary && !values.username.trim()) {
    context.addIssue({ code: "custom", path: ["username"], message: "required" });
  }
  if (values.routeType === "jumpHost" && !values.jumpProfileId) context.addIssue({ code: "custom", path: ["jumpProfileId"], message: "required" });
  if (values.routeType === "bastion" && !values.bastionProvider.trim()) context.addIssue({ code: "custom", path: ["bastionProvider"], message: "required" });
  if (isJumpServer || isBoundary) {
    const api = values.bastionApiBaseUrl.trim();
    if (!api) context.addIssue({ code: "custom", path: ["bastionApiBaseUrl"], message: "required" });
    else if (!/^https?:\/\/[^/\s]+/i.test(api)) context.addIssue({ code: "custom", path: ["bastionApiBaseUrl"], message: "invalid" });
  }
  if (isTeleport && values.port < 1) {
    context.addIssue({ code: "custom", path: ["port"], message: "required" });
  }
  if (isJumpServer || values.authMethod !== "privateKey") return;
  if (values.keySourceType === "file" && !values.keyPath.trim()) context.addIssue({ code: "custom", path: ["keyPath"], message: "required" });
  if (values.keySourceType === "vault" && !values.keyId) context.addIssue({ code: "custom", path: ["keyId"], message: "required" });
});

type Values = z.infer<typeof schema>;
type RouteType = Values["routeType"];
type FieldProps = { id: string; label: string; error?: string; children: ReactNode; hint?: string };

function FormField({ id, label, error, children, hint }: FieldProps) {
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>{label}</Label>
      {children}
      {error
        ? <p id={`${id}-error`} role="alert" className="flex items-center gap-1.5 text-xs text-red-500"><AlertCircle size={12} aria-hidden="true" />{error}</p>
        : hint && <p id={`${id}-hint`} className="text-xs leading-relaxed text-[hsl(var(--muted))]">{hint}</p>}
    </div>
  );
}

function FieldGroup({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="space-y-3" aria-label={title}>
      <h3 className="text-xs font-semibold uppercase tracking-wide text-[hsl(var(--muted))]">{title}</h3>
      <div className="space-y-4">{children}</div>
    </section>
  );
}

export function ProfileDialog({ profile, initialGroupId = null, onClose }: { profile?: ServerProfile; initialGroupId?: string | null; onClose: () => void }) {
  const { t } = useTranslation();
  const groups = useCatalogStore((state) => state.groups);
  const profiles = useCatalogStore((state) => state.profiles);
  const createProfile = useCatalogStore((state) => state.createProfile);
  const updateProfile = useCatalogStore((state) => state.updateProfile);
  const boundaryCliPathGlobal = useSettingsStore((state) => state.boundaryCliPath);
  const teleportCliPathGlobal = useSettingsStore((state) => state.teleportCliPath);
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
      routeType: profile?.connectionRoute.type === "jumpHost" ? "jumpHost" : profile?.connectionRoute.type === "bastion" ? "bastion" : "direct",
      jumpProfileId: profile?.connectionRoute.type === "jumpHost" ? profile.connectionRoute.profileId : "",
      bastionProvider: profile?.connectionRoute.type === "bastion" ? profile.connectionRoute.provider : "jumpserver",
      bastionApiBaseUrl: profile?.connectionRoute.type === "bastion" ? (profile.connectionRoute.apiBaseUrl ?? "") : "",
      bastionOrgId: profile?.connectionRoute.type === "bastion" ? (profile.connectionRoute.orgId ?? "") : "",
      bastionAssetId: profile?.connectionRoute.type === "bastion" ? profile.connectionRoute.assetId : "",
      bastionAccountId: profile?.connectionRoute.type === "bastion" ? (profile.connectionRoute.accountId ?? "") : "",
      teleportClusterName: profile?.connectionRoute.type === "bastion" ? (profile.connectionRoute.clusterName ?? "") : "",
      teleportInsecure: profile?.connectionRoute.type === "bastion" ? Boolean(profile.connectionRoute.insecure) : false,
      keySourceType: profile?.keySource?.type ?? (matchMedia("(pointer: coarse)").matches ? "vault" : "file"),
      keyPath: profile?.keySource?.type === "file" ? profile.keySource.path : "",
      keyId: profile?.keySource?.type === "vault" ? profile.keySource.keyId : "",
    },
  });
  const authMethod = watch("authMethod");
  const keySourceType = watch("keySourceType");
  const routeType = watch("routeType");
  const bastionProvider = watch("bastionProvider");
  const isJumpServer = routeType === "bastion" && bastionProvider === "jumpserver";
  const isBoundary = routeType === "bastion" && bastionProvider === "boundary";
  const isTeleport = routeType === "bastion" && bastionProvider === "teleport";
  const needsHelperCli = routeType === "bastion" && (bastionProvider === "teleport" || bastionProvider === "boundary");
  const globalHelperCliPath = isBoundary
    ? boundaryCliPathGlobal.trim()
    : bastionProvider === "teleport"
      ? teleportCliPathGlobal.trim()
      : "";
  const helperCliConfigured = Boolean(globalHelperCliPath);
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

  const applyRouteDefaults = (next: RouteType) => {
    setValue("routeType", next, { shouldDirty: true });
    if (next === "direct" || next === "jumpHost") {
      setValue("port", 22, { shouldDirty: true });
      return;
    }
    setValue("authMethod", "password", { shouldDirty: true });
    if (bastionProvider === "jumpserver") setValue("port", 2222, { shouldDirty: true });
    if (bastionProvider === "boundary") setValue("port", 9200, { shouldDirty: true });
    if (bastionProvider === "teleport") setValue("port", 3080, { shouldDirty: true });
  };

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
  const openHelperCliSettings = () => {
    window.dispatchEvent(
      new CustomEvent<{ section: SettingsSection }>("runory:open-settings", {
        detail: { section: "helpers" },
      }),
    );
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
      : values.routeType === "bastion"
        ? {
            type: "bastion" as const,
            bastionId: profile?.connectionRoute.type === "bastion" ? profile.connectionRoute.bastionId : crypto.randomUUID(),
            provider: values.bastionProvider.trim() || "jumpserver",
            assetId: values.bastionAssetId.trim(),
            accountId: values.bastionAccountId.trim() || undefined,
            apiBaseUrl: (values.bastionProvider === "jumpserver" || values.bastionProvider === "boundary")
              ? values.bastionApiBaseUrl.trim().replace(/\/+$/, "") || undefined
              : undefined,
            orgId: values.bastionProvider === "jumpserver"
              ? values.bastionOrgId.trim() || undefined
              : undefined,
            // CLI path is configured globally in Settings → Helper CLI.
            cliPath: undefined,
            clusterName: values.bastionProvider === "teleport"
              ? values.teleportClusterName.trim() || undefined
              : undefined,
            insecure: values.bastionProvider === "teleport" ? values.teleportInsecure : undefined,
          }
        : { type: "direct" as const };
    let host = values.host.trim();
    let port = values.port;
    let username = values.username.trim();
    let authMethod = values.authMethod;
    // Bastion auth is provider-specific (Token / Access Key / SSO); profile authMethod is unused.
    if (values.routeType === "bastion") {
      authMethod = "password";
    }
    if (values.routeType === "bastion" && values.bastionProvider === "jumpserver") {
      const api = values.bastionApiBaseUrl.trim();
      try {
        const parsed = new URL(api.includes("://") ? api : `https://${api}`);
        host = parsed.hostname;
        port = 2222;
      } catch {
        port = 2222;
      }
      // Profile.username is required by catalog validation but unused for JumpServer auth.
      if (!username) username = values.name.trim();
    }
    if (values.routeType === "bastion" && values.bastionProvider === "boundary") {
      const api = values.bastionApiBaseUrl.trim();
      try {
        const parsed = new URL(api.includes("://") ? api : `http://${api}`);
        host = parsed.hostname;
        port = parsed.port
          ? Number(parsed.port)
          : parsed.protocol === "https:"
            ? 443
            : 9200;
      } catch {
        port = 9200;
      }
      if (!username) username = values.name.trim();
    }
    if (values.routeType === "bastion" && values.bastionProvider === "teleport") {
      // Host = Teleport Proxy host; Port = proxy port (e.g. 3080); Username = Teleport user.
      if (!port) port = 3080;
    }
    const request = {
      name: values.name.trim(),
      host,
      port,
      username,
      groupId: values.groupId || null,
      authMethod,
      keySource: authMethod === "privateKey" ? keySource : undefined,
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

  const renderIdentityFields = () => (
    <FieldGroup title={t("profile.identitySection")}>
      <div className="grid gap-4 sm:grid-cols-2">
        <FormField id="profile-name" label={t("profile.name")} error={errors.name ? t("validation.profileName") : undefined}>
          <Input id="profile-name" autoFocus={!profile} autoComplete="off" placeholder={t("profile.namePlaceholder")} aria-invalid={Boolean(errors.name)} aria-describedby={errors.name ? "profile-name-error" : undefined} disabled={busy} {...register("name")} />
        </FormField>
        <FormField id="profile-group" label={t("profile.group")}>
          <Controller control={control} name="groupId" render={({ field }) => (
            <SearchableCombobox
              id="profile-group"
              label={t("profile.group")}
              value={field.value}
              options={groupOptions}
              placeholder={t("sidebar.ungrouped")}
              searchPlaceholder={t("profile.groupSearchPlaceholder")}
              emptyMessage={t("profile.noMatchingGroups")}
              disabled={busy}
              onValueChange={field.onChange}
            />
          )} />
        </FormField>
      </div>
    </FieldGroup>
  );

  const renderSshEndpointFields = () => (
    <>
      <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_112px]">
        <FormField id="profile-host" label={t("profile.host")} error={errors.host ? t("validation.profileHost") : undefined}>
          <Input id="profile-host" autoCapitalize="none" autoCorrect="off" spellCheck={false} placeholder={t("profile.hostPlaceholder")} aria-invalid={Boolean(errors.host)} aria-describedby={errors.host ? "profile-host-error" : undefined} disabled={busy} {...register("host")} />
        </FormField>
        <FormField id="profile-port" label={t("profile.port")} error={errors.port ? t("validation.profilePort") : undefined}>
          <Input id="profile-port" type="number" inputMode="numeric" min={1} max={65535} aria-invalid={Boolean(errors.port)} aria-describedby={errors.port ? "profile-port-error" : undefined} disabled={busy} {...register("port", { valueAsNumber: true })} />
        </FormField>
      </div>
      <FormField id="profile-username" label={t("profile.username")} error={errors.username ? t("validation.profileUsername") : undefined}>
        <Input id="profile-username" autoCapitalize="none" autoCorrect="off" spellCheck={false} autoComplete="username" placeholder={t("profile.usernamePlaceholder")} aria-invalid={Boolean(errors.username)} aria-describedby={errors.username ? "profile-username-error" : undefined} disabled={busy} {...register("username")} />
      </FormField>
    </>
  );

  const renderAuthFields = () => (
    <div className="space-y-4">
      <FormField id="profile-auth" label={t("profile.authMethod")}>
        <Controller control={control} name="authMethod" render={({ field }) => (
          <Select value={field.value} disabled={busy} onValueChange={field.onChange}>
            <SelectTrigger id="profile-auth"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="password">{t("profile.password")}</SelectItem>
              <SelectItem value="privateKey">{t("profile.privateKey")}</SelectItem>
            </SelectContent>
          </Select>
        )} />
      </FormField>
      {authMethod === "privateKey" && (
        <div className="space-y-4 rounded-lg border border-[hsl(var(--border-soft))] bg-[hsl(var(--elevated)/.35)] p-3">
          <FormField id="profile-key-storage" label={t("profile.keyStorage")}>
            <Controller control={control} name="keySourceType" render={({ field }) => (
              <Select value={field.value} disabled={busy} onValueChange={field.onChange}>
                <SelectTrigger id="profile-key-storage"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="file">{t("profile.keyFile")}</SelectItem>
                  <SelectItem value="vault">{t("profile.keyVault")}</SelectItem>
                </SelectContent>
              </Select>
            )} />
          </FormField>
          {keySourceType === "file" ? (
            <FormField id="profile-key-path" label={t("profile.privateKey")} error={errors.keyPath ? t("validation.profilePrivateKey") : undefined} hint={t("profile.privateKeyHint")}>
              <div className="flex gap-2">
                <Input id="profile-key-path" className="min-w-0 font-mono" readOnly placeholder={t("profile.keyPathPlaceholder")} aria-invalid={Boolean(errors.keyPath)} aria-describedby={errors.keyPath ? "profile-key-path-error" : "profile-key-path-hint"} disabled={busy} {...register("keyPath")} />
                <Button type="button" variant="secondary" disabled={busy} onClick={() => void browse()}><FolderOpen size={16} aria-hidden="true" /><span className="hidden sm:inline">{t("profile.browse")}</span></Button>
              </div>
            </FormField>
          ) : (
            <FormField id="profile-key-vault" label={t("profile.privateKey")} error={errors.keyId ? t("validation.profilePrivateKey") : undefined} hint={t("profile.vaultKeyHint")}>
              <input type="hidden" {...register("keyId")} />
              <Button id="profile-key-vault" type="button" className="max-w-full justify-start" variant="secondary" disabled={busy} aria-describedby={errors.keyId ? "profile-key-vault-error" : "profile-key-vault-hint"} onClick={() => void importKey()}>
                {importing ? <Loader2 size={16} className="animate-spin" aria-hidden="true" /> : keyLabel ? <Check size={16} className="text-emerald-500" aria-hidden="true" /> : <KeyRound size={16} aria-hidden="true" />}
                <span className="truncate">{keyLabel || t("profile.importToVault")}</span>
              </Button>
            </FormField>
          )}
        </div>
      )}
    </div>
  );

  const renderHelperCliWarning = () => (
    needsHelperCli && !helperCliConfigured ? (
      <div className="space-y-3 rounded-lg border border-amber-500/30 bg-amber-500/5 p-3">
        <p className="text-sm leading-relaxed text-[hsl(var(--muted))]">
          {t(isBoundary ? "bastion.helperCliMissingBoundary" : "bastion.helperCliMissingTeleport")}
        </p>
        <Button type="button" variant="secondary" size="sm" disabled={busy} onClick={openHelperCliSettings}>
          {t("bastion.openHelperCliSettings")}
        </Button>
      </div>
    ) : null
  );

  return (
    <Dialog
      open
      onOpenChange={(next) => {
        if (!next) close();
      }}
    >
      <DialogContent
        className="sm:max-w-xl"
        closeDisabled={busy}
        onPointerDownOutside={(event) => {
          if (busy) event.preventDefault();
        }}
        onEscapeKeyDown={(event) => {
          if (busy) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>{t(profile ? "profile.editTitle" : "profile.createTitle")}</DialogTitle>
        </DialogHeader>
        <form className="flex min-h-0 flex-1 flex-col" noValidate onSubmit={submit}>
          <div className="-mx-4 max-h-[55vh] space-y-5 overflow-y-auto px-4 py-1">
            <div className="space-y-2">
              <Tabs
                value={routeType}
                onValueChange={(value) => applyRouteDefaults(value as RouteType)}
              >
                <TabsList aria-label={t("profile.connectionRoute")} id="profile-route">
                  <TabsTrigger value="direct" disabled={busy}>{t("profile.routeTabDirect")}</TabsTrigger>
                  <TabsTrigger value="jumpHost" disabled={busy}>{t("profile.routeTabJumpHost")}</TabsTrigger>
                  <TabsTrigger value="bastion" disabled={busy}>{t("profile.routeTabBastion")}</TabsTrigger>
                </TabsList>

                <div className="mt-4 space-y-5">
                  {renderIdentityFields()}
                </div>

                <TabsContent value="direct" className="mt-5 space-y-5">
                  <FieldGroup title={t("profile.endpointSection")}>{renderSshEndpointFields()}</FieldGroup>
                  {renderAuthFields()}
                </TabsContent>

                <TabsContent value="jumpHost" className="mt-5 space-y-5">
                  <FormField
                    id="profile-jump-host"
                    label={t("profile.jumpHost")}
                    error={errors.jumpProfileId ? t("validation.profileJumpHost") : undefined}
                    hint={t(jumpCandidates.length ? "profile.jumpHostHint" : "profile.noJumpHosts")}
                  >
                    <Controller control={control} name="jumpProfileId" render={({ field }) => (
                      <SearchableCombobox
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
                      />
                    )} />
                  </FormField>
                  <FieldGroup title={t("profile.endpointSection")}>{renderSshEndpointFields()}</FieldGroup>
                  {renderAuthFields()}
                </TabsContent>

                <TabsContent value="bastion" className="mt-5 space-y-5">
                  <FieldGroup title={t("profile.bastionSection")}>
                    <FormField id="profile-bastion-provider" label={t("bastion.provider")} error={errors.bastionProvider ? t("validation.bastionProvider") : undefined}>
                      <Controller control={control} name="bastionProvider" render={({ field }) => (
                        <Select
                          value={field.value}
                          disabled={busy}
                          onValueChange={(value) => {
                            field.onChange(value);
                            setValue("authMethod", "password", { shouldDirty: true });
                            if (value === "jumpserver") setValue("port", 2222, { shouldDirty: true });
                            if (value === "teleport") setValue("port", 3080, { shouldDirty: true });
                            if (value === "boundary") setValue("port", 9200, { shouldDirty: true });
                          }}
                        >
                          <SelectTrigger id="profile-bastion-provider"><SelectValue /></SelectTrigger>
                          <SelectContent>
                            <SelectItem value="jumpserver">{t("bastion.providerJumpServer")}</SelectItem>
                            <SelectItem value="teleport">{t("bastion.providerTeleport")}</SelectItem>
                            <SelectItem value="boundary">{t("bastion.providerBoundary")}</SelectItem>
                          </SelectContent>
                        </Select>
                      )} />
                    </FormField>

                    {isJumpServer && (
                      <>
                        <FormField
                          id="profile-bastion-api"
                          label={t("bastion.apiBaseUrl")}
                          error={errors.bastionApiBaseUrl ? t(errors.bastionApiBaseUrl.message === "invalid" ? "validation.bastionApiBaseUrlInvalid" : "validation.bastionApiBaseUrl") : undefined}
                        >
                          <Input id="profile-bastion-api" autoCapitalize="none" spellCheck={false} disabled={busy} placeholder="https://jms.example.com" {...register("bastionApiBaseUrl")} />
                        </FormField>
                        <FormField id="profile-bastion-org" label={t("bastion.orgId")} hint={t("bastion.orgIdHint")}>
                          <Input id="profile-bastion-org" autoCapitalize="none" spellCheck={false} disabled={busy} placeholder={t("bastion.optionalPlaceholder")} {...register("bastionOrgId")} />
                        </FormField>
                        <div className="grid gap-4 sm:grid-cols-2">
                          <FormField id="profile-bastion-asset" label={t("bastion.assetId")} hint={t("bastion.assetIdHint")}>
                            <Input id="profile-bastion-asset" autoCapitalize="none" spellCheck={false} disabled={busy} placeholder={t("bastion.optionalPlaceholder")} {...register("bastionAssetId")} />
                          </FormField>
                          <FormField id="profile-bastion-account" label={t("bastion.accountId")} hint={t("bastion.accountIdHint")}>
                            <Input id="profile-bastion-account" autoCapitalize="none" spellCheck={false} disabled={busy} placeholder={t("bastion.optionalPlaceholder")} {...register("bastionAccountId")} />
                          </FormField>
                        </div>
                      </>
                    )}

                    {isBoundary && (
                      <>
                        <FormField
                          id="profile-bastion-api"
                          label={t("bastion.boundaryControllerUrl")}
                          error={errors.bastionApiBaseUrl ? t(errors.bastionApiBaseUrl.message === "invalid" ? "validation.bastionApiBaseUrlInvalid" : "validation.bastionBoundaryControllerUrl") : undefined}
                          hint={t("bastion.boundaryControllerUrlHint")}
                        >
                          <Input id="profile-bastion-api" autoCapitalize="none" spellCheck={false} disabled={busy} placeholder="http://192.168.1.10:9200" {...register("bastionApiBaseUrl")} />
                        </FormField>
                        <div className="grid gap-4 sm:grid-cols-2">
                          <FormField id="profile-bastion-asset" label={t("bastion.boundaryTargetId")} hint={t("bastion.boundaryBoundAssetHint")}>
                            <Input id="profile-bastion-asset" autoCapitalize="none" spellCheck={false} disabled={busy} placeholder="ttcp_..." {...register("bastionAssetId")} />
                          </FormField>
                          <FormField id="profile-bastion-account" label={t("bastion.boundTargetAccount")} hint={t("bastion.boundaryBoundAccountHint")}>
                            <Input id="profile-bastion-account" autoCapitalize="none" spellCheck={false} disabled={busy} placeholder={t("bastion.optionalPlaceholder")} {...register("bastionAccountId")} />
                          </FormField>
                        </div>
                        {renderHelperCliWarning()}
                      </>
                    )}

                    {isTeleport && (
                      <>
                        <p className="text-xs text-[hsl(var(--muted))]">{t("bastion.teleportProfileHint")}</p>
                        {renderSshEndpointFields()}
                        <div className="grid gap-4 sm:grid-cols-2">
                          <FormField id="profile-bastion-asset" label={t("bastion.assetId")} hint={t("bastion.helperAssetIdHint")}>
                            <Input id="profile-bastion-asset" autoCapitalize="none" spellCheck={false} disabled={busy} placeholder="teleport-node01" {...register("bastionAssetId")} />
                          </FormField>
                          <FormField id="profile-bastion-account" label={t("bastion.osLogin")} hint={t("bastion.osLoginHint")}>
                            <Input id="profile-bastion-account" autoCapitalize="none" spellCheck={false} disabled={busy} placeholder="root" {...register("bastionAccountId")} />
                          </FormField>
                        </div>
                        <FormField id="profile-teleport-cluster" label={t("bastion.teleportCluster")} hint={t("bastion.teleportClusterHint")}>
                          <Input id="profile-teleport-cluster" autoCapitalize="none" spellCheck={false} disabled={busy} placeholder="teleport.local" {...register("teleportClusterName")} />
                        </FormField>
                        <label className="flex items-start gap-2 text-sm">
                          <input className="mt-1" type="checkbox" disabled={busy} {...register("teleportInsecure")} />
                          <span>
                            <span className="font-medium">{t("bastion.teleportInsecure")}</span>
                            <span className="mt-0.5 block text-xs text-[hsl(var(--muted))]">{t("bastion.teleportInsecureHint")}</span>
                          </span>
                        </label>
                        {renderHelperCliWarning()}
                      </>
                    )}
                  </FieldGroup>
                </TabsContent>
              </Tabs>
            </div>

            {failure && (
              <div role="alert" className="flex items-start gap-2 rounded-md border border-red-500/30 bg-red-500/5 p-3 text-sm text-red-500">
                <AlertCircle size={16} className="mt-0.5 shrink-0" aria-hidden="true" />
                <span>{t("errors.saveFailed")}</span>
              </div>
            )}
          </div>

          <DialogFooter>
            <DialogClose asChild>
              <Button type="button" variant="secondary" disabled={busy}>{t("common.cancel")}</Button>
            </DialogClose>
            <Button type="submit" disabled={busy}>
              {isSubmitting && <Loader2 size={16} className="animate-spin" aria-hidden="true" />}
              {isSubmitting ? t("common.saving") : t("common.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
