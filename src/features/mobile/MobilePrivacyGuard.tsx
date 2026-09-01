import { authenticate, checkStatus } from "@tauri-apps/plugin-biometric";
import { Fingerprint, ShieldAlert } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";

const mobileUserAgent = () => /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);

export function MobilePrivacyGuard() {
  const { t } = useTranslation();
  const enabled = useRef(false);
  const [locked, setLocked] = useState(false);
  const [failure, setFailure] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!mobileUserAgent()) return;
    void checkStatus().then((status) => { enabled.current = status.isAvailable; }).catch(() => { enabled.current = false; });
    const visibility = () => {
      if (document.hidden) {
        document.documentElement.dataset.privacy = "locked";
        if (enabled.current) setLocked(true);
      } else if (!enabled.current) {
        document.documentElement.removeAttribute("data-privacy");
      }
    };
    document.addEventListener("visibilitychange", visibility);
    return () => document.removeEventListener("visibilitychange", visibility);
  }, []);

  const unlock = async () => {
    setBusy(true); setFailure(false);
    try {
      await authenticate(t("mobile.biometricReason"), {
        allowDeviceCredential: true,
        cancelTitle: t("common.cancel"),
        fallbackTitle: t("mobile.useDeviceCredential"),
        title: t("mobile.unlockRunory"),
        subtitle: t("mobile.biometricSubtitle"),
        confirmationRequired: false,
      });
      document.documentElement.removeAttribute("data-privacy");
      setLocked(false);
    } catch { setFailure(true); } finally { setBusy(false); }
  };

  if (!locked) return null;
  return <div className="fixed inset-0 z-[100] grid place-items-center bg-[hsl(var(--background))] p-6" role="dialog" aria-modal="true" aria-label={t("mobile.privacyLocked")}>
    <div className="w-full max-w-sm text-center"><div className="mx-auto grid h-16 w-16 place-items-center rounded-2xl bg-blue-500/10 text-blue-500"><Fingerprint size={34} /></div><h1 className="mt-5 text-xl font-semibold">{t("mobile.privacyLocked")}</h1><p className="mt-2 text-sm text-[hsl(var(--secondary))]">{t("mobile.privacyDescription")}</p><Button className="mt-6 h-12 w-full" disabled={busy} onClick={() => void unlock()}><Fingerprint size={18} />{t("mobile.unlock")}</Button>{failure && <p className="mt-3 flex items-center justify-center gap-2 text-sm text-red-500"><ShieldAlert size={15} />{t("mobile.biometricFailed")}</p>}</div>
  </div>;
}
