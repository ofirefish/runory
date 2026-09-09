import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, CreditCard, Minus, Moon, Square, Sun, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettingsStore } from "../../stores/settings-store";

const appWindow = () => isTauri() ? getCurrentWindow() : null;

export function WindowControls({ onOpenPricing }: { onOpenPricing: () => void }) {
  const { t } = useTranslation();
  const [maximized, setMaximized] = useState(false);
  const [systemDark, setSystemDark] = useState(() => window.matchMedia("(prefers-color-scheme: dark)").matches);
  const theme = useSettingsStore((state) => state.theme);
  const setTheme = useSettingsStore((state) => state.setTheme);
  const macOS = navigator.userAgent.includes("Macintosh");
  const dark = theme === "dark" || (theme === "system" && systemDark);

  useEffect(() => {
    const current = appWindow();
    if (!current) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void current.isMaximized().then((value) => { if (!disposed) setMaximized(value); }).catch(() => undefined);
    void current.onResized(() => { void current.isMaximized().then((value) => { if (!disposed) setMaximized(value); }).catch(() => undefined); }).then((stop) => { unlisten = stop; });
    return () => { disposed = true; unlisten?.(); };
  }, []);
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const update = () => setSystemDark(media.matches);
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);

  const minimize = () => { void appWindow()?.minimize(); };
  const toggleMaximize = () => { void appWindow()?.toggleMaximize().then(() => appWindow()?.isMaximized()).then((value) => { if (typeof value === "boolean") setMaximized(value); }); };
  const close = () => { void appWindow()?.close(); };
  const toggleTheme = () => setTheme(dark ? "light" : "dark");
  const pricingButton = <button type="button" className="window-pricing-button" aria-label={t("window.openPricing")} title={t("window.openPricing")} onClick={onOpenPricing}><CreditCard size={16} /></button>;
  const themeToggle = <button type="button" className="window-theme-toggle" aria-label={t(dark ? "window.switchToLight" : "window.switchToDark")} title={t(dark ? "window.switchToLight" : "window.switchToDark")} onClick={toggleTheme}>{dark ? <Sun size={16} /> : <Moon size={16} />}</button>;

  if (macOS) return <div className="window-controls macos" data-no-drag>
    <button type="button" className="traffic-close" aria-label={t("window.close")} title={t("window.close")} onClick={close} />
    <button type="button" className="traffic-minimize" aria-label={t("window.minimize")} title={t("window.minimize")} onClick={minimize} />
    <button type="button" className="traffic-maximize" aria-label={maximized ? t("window.restore") : t("window.maximize")} title={maximized ? t("window.restore") : t("window.maximize")} onClick={toggleMaximize} />
    {pricingButton}
    {themeToggle}
  </div>;

  return <div className="window-controls" data-no-drag>
    {pricingButton}
    {themeToggle}
    <button type="button" aria-label={t("window.minimize")} title={t("window.minimize")} onClick={minimize}><Minus size={16} strokeWidth={1.6} /></button>
    <button type="button" aria-label={maximized ? t("window.restore") : t("window.maximize")} title={maximized ? t("window.restore") : t("window.maximize")} onClick={toggleMaximize}>{maximized ? <Copy className="restore-icon" size={13} strokeWidth={1.6} /> : <Square size={13} strokeWidth={1.6} />}</button>
    <button type="button" className="window-close" aria-label={t("window.close")} title={t("window.close")} onClick={close}><X size={17} strokeWidth={1.6} /></button>
  </div>;
}
