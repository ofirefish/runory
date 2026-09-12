import { Bot, Cloud, Palette, ShieldCheck, Terminal, Trash2, UserCircle, type LucideIcon } from "lucide-react";
import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Input } from "../../components/ui/input";
import { credentialStatus, initializeVault, listKnownHosts, lockVault, removeKnownHost, unlockVault, unlockVaultWithPlatform } from "../../lib/tauri/ssh";
import { vaultUnlockAction } from "../../lib/vault-unlock";
import { useSettingsStore } from "../../stores/settings-store";
import type { KnownHost } from "../../types/session";
import type { CredentialStatus } from "../../types/session";
import { AgentModelSettings } from "./AgentModelSettings";
import { AppUpdatePanel } from "./AppUpdatePanel";
import { HelperCliSettings } from "./HelperCliSettings";
import { SettingsSelectField } from "./SettingsSelectField";

const AccountSettings = lazy(() => import("./AccountSettings").then((module) => ({ default: module.AccountSettings })));
const CloudPanel = lazy(() => import("./CloudPanel").then((module) => ({ default: module.CloudPanel })));

export type SettingsSection = "general" | "helpers" | "account" | "agent" | "security" | "cloud";

const sections: { id: SettingsSection; icon: LucideIcon; label: string }[] = [
  { id: "general", icon: Palette, label: "settings.section.general" },
  { id: "account", icon: UserCircle, label: "settings.section.account" },
  { id: "agent", icon: Bot, label: "settings.section.agent" },
  { id: "security", icon: ShieldCheck, label: "settings.section.security" },
  { id: "cloud", icon: Cloud, label: "settings.section.cloud" },
  { id: "helpers", icon: Terminal, label: "settings.section.helpers" },
];

export function SettingsPanel({ onClose, initialSection = "general" }: { onClose: () => void; initialSection?: SettingsSection }) {
  const { t, i18n } = useTranslation();
  const { theme, language, saving, persistenceError, setTheme, setLanguage } = useSettingsStore();
  const [knownHosts, setKnownHosts] = useState<KnownHost[]>([]);
  const [vault, setVault] = useState<CredentialStatus | null>(null);
  const [vaultFailure, setVaultFailure] = useState(false);
  const vaultPassword = useRef<HTMLInputElement>(null);
  const vaultPasswordConfirmation = useRef<HTMLInputElement>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const [activeSection, setActiveSection] = useState<SettingsSection>(initialSection);
  useEffect(() => { setActiveSection(initialSection); }, [initialSection]);
  useEffect(() => { void i18n.changeLanguage(language); document.documentElement.lang = language; }, [i18n, language]);
  useEffect(() => { void listKnownHosts().then(setKnownHosts).catch(() => setLoadFailed(true)); }, []);
  useEffect(() => { void credentialStatus().then(setVault).catch(() => setVaultFailure(true)); }, []);
  const remove = async (knownHost: KnownHost) => { try { await removeKnownHost(knownHost.routeScope, knownHost.host, knownHost.port); setKnownHosts((hosts) => hosts.filter((host) => host.routeScope !== knownHost.routeScope || host.host !== knownHost.host || host.port !== knownHost.port)); } catch { setLoadFailed(true); } };
  const unlockWithPassword = async () => { const input = vaultPassword.current; const confirmation = vaultPasswordConfirmation.current; if (!input?.value || (!vault?.vaultInitialized && input.value !== confirmation?.value)) { setVaultFailure(true); return; } setVaultFailure(false); try { await unlockVault(input.value); input.value = ""; if (confirmation) confirmation.value = ""; setVault(await credentialStatus()); } catch { input.value = ""; if (confirmation) confirmation.value = ""; setVaultFailure(true); } };
  const unlockOrInitialize = async () => { setVaultFailure(false); try { const action = vaultUnlockAction(vault); if (action === "unlock-password") { await unlockWithPassword(); return; } if (action === "unlock-platform") await unlockVaultWithPlatform(); else if (action === "initialize-platform") await initializeVault(); setVault(await credentialStatus()); } catch { setVaultFailure(true); } };
  const lock = async () => { setVaultFailure(false); try { await lockVault(); setVault(await credentialStatus()); } catch { setVaultFailure(true); } };
  const heading = t(`settings.section.${activeSection}`);
  return <DialogShell title={t("sidebar.settings")} onClose={onClose} size="wide" panelClassName="settings-dialog-panel" contentClassName="settings-layout">
    <nav className="settings-navigation" aria-label={t("settings.navigation")}>
      <div className="settings-navigation-items">
        {sections.map(({ id, icon: Icon, label }) => <Button key={id} type="button" variant="ghost" className="settings-navigation-item h-auto justify-start" data-active={activeSection === id} aria-current={activeSection === id ? "page" : undefined} onClick={() => setActiveSection(id)}><Icon size={16} /><span>{t(label)}</span></Button>)}
      </div>
    </nav>
    <main className="settings-content">
      <header className="settings-content-header"><h3>{heading}</h3><p>{t(`settings.section.${activeSection}Hint`)}</p></header>
      {activeSection === "general" && <div className="settings-group">
        <SettingsSelectField className="settings-field" label={t("settings.theme")} value={theme} onValueChange={(value) => void setTheme(value)} options={(["system", "light", "dark"] as const).map((value) => ({ value, label: t(`settings.${value}`) }))} />
        <SettingsSelectField className="settings-field" label={t("settings.language")} value={language} onValueChange={(value) => void setLanguage(value)} options={[{ value: "en-US", label: t("settings.english") }, { value: "zh-CN", label: t("settings.chinese") }]} />
        {saving && <p role="status" className="text-xs text-[hsl(var(--muted))]">{t("settings.saving")}</p>}
        {persistenceError && <p role="alert" className="text-xs text-red-500">{t("settings.persistenceError")}</p>}
        <AppUpdatePanel />
      </div>}
      {activeSection === "helpers" && <HelperCliSettings />}
      {activeSection === "account" && <div className="settings-section-reset"><Suspense fallback={<p className="text-xs text-[hsl(var(--muted))]">{t("common.loading")}</p>}><AccountSettings onOpenCloud={() => setActiveSection("cloud")} /></Suspense></div>}
      {activeSection === "agent" && <div className="settings-section-reset"><AgentModelSettings /></div>}
      {activeSection === "security" && <div className="settings-stack">
        <section className="settings-card"><h4>{t("settings.credentialVault")}</h4><p>{vault?.vaultInitialized ? (vault.vaultUnlocked ? t(vault.platformUnlockConfigured ? "settings.vaultSystemProtected" : "settings.vaultUnlocked") : t(vault.platformUnlockConfigured && vault.platformUnlockAvailable ? "settings.vaultLocked" : vault.platformUnlockAvailable ? "settings.vaultMigrationRequired" : "settings.vaultPasswordRequired")) : t("settings.vaultNotCreated")}</p>{vault && !vault.vaultUnlocked && vault.platformUnlockConfigured && vault.platformUnlockAvailable && <Button className="mt-3" size="sm" onClick={() => void unlockOrInitialize()}>{t("settings.unlockVault")}</Button>}{vault && !vault.vaultUnlocked && vault.vaultInitialized && (!vault.platformUnlockConfigured || !vault.platformUnlockAvailable) && <div className="mt-3 space-y-2"><p className="text-xs text-[hsl(var(--muted))]">{t(vault.platformUnlockAvailable ? "settings.vaultMigrationHint" : "settings.vaultPasswordFallbackHint")}</p><Input ref={vaultPassword} type="password" autoComplete="current-password" aria-label={t("settings.vaultPassword")} placeholder={t("settings.vaultPassword")} /><Button size="sm" onClick={() => void unlockWithPassword()}>{t(vault.platformUnlockAvailable ? "settings.migrateVault" : "settings.unlockVault")}</Button></div>}{vault && !vault.vaultUnlocked && !vault.vaultInitialized && vault.platformUnlockAvailable && <div className="mt-3 space-y-2"><p className="text-xs text-[hsl(var(--muted))]">{t("settings.createPlatformVaultHint")}</p><Button size="sm" onClick={() => void unlockOrInitialize()}>{t("settings.createVault")}</Button></div>}{vault && !vault.vaultUnlocked && !vault.vaultInitialized && !vault.platformUnlockAvailable && <div className="mt-3 space-y-2"><Input ref={vaultPassword} type="password" autoComplete="new-password" aria-label={t("settings.vaultPassword")} placeholder={t("settings.vaultPassword")} /><Input ref={vaultPasswordConfirmation} type="password" autoComplete="new-password" aria-label={t("connection.confirmVaultPassword")} placeholder={t("connection.confirmVaultPassword")} /><Button size="sm" onClick={() => void unlockWithPassword()}>{t("settings.createVault")}</Button></div>}{vault?.vaultUnlocked && <Button className="mt-3" size="sm" variant="secondary" onClick={() => void lock()}>{t("settings.lockVault")}</Button>}{vaultFailure && <p className="mt-2 text-xs text-red-500">{t("settings.vaultError")}</p>}</section>
        <section className="settings-card"><h4>{t("settings.trustedHosts")}</h4><p>{t("settings.trustedHostsHint")}</p>{knownHosts.length === 0 && !loadFailed && <p className="mt-3">{t("settings.noTrustedHosts")}</p>}<div className="mt-3 space-y-2">{knownHosts.map((knownHost) => <div key={`${knownHost.routeScope}:${knownHost.host}:${knownHost.port}`} className="flex items-start gap-2 rounded-md border p-2"><div className="min-w-0 flex-1"><div className="truncate text-xs font-medium">{knownHost.host}:{knownHost.port} · {t(knownHost.routeScope === "direct" ? "profile.routeDirect" : "profile.routeJumpHost")}</div><div className="mt-1 truncate font-mono text-[10px] text-[hsl(var(--muted))]" title={knownHost.fingerprint}>{knownHost.fingerprint}</div></div><Button variant="ghost" size="icon" className="h-7 w-7 shrink-0 text-red-500" aria-label={t("settings.removeTrustedHost", { host: knownHost.host })} onClick={() => void remove(knownHost)}><Trash2 size={14} /></Button></div>)}</div>{loadFailed && <p className="mt-2 text-xs text-red-500">{t("settings.trustedHostsError")}</p>}</section>
      </div>}
      {activeSection === "cloud" && <div className="settings-section-reset"><Suspense fallback={<p className="text-xs text-[hsl(var(--muted))]">{t("common.loading")}</p>}><CloudPanel onOpenAccount={() => setActiveSection("account")} /></Suspense></div>}
    </main>
  </DialogShell>;
}
