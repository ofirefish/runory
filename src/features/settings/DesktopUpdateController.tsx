import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useAppUpdateStore } from "../../stores/app-update-store";

const AUTOMATIC_CHECK_DELAY_MS = 10_000;

export function DesktopUpdateController({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { t } = useTranslation();
  const phase = useAppUpdateStore((state) => state.phase);
  const version = useAppUpdateStore((state) => state.update?.version);
  const initialize = useAppUpdateStore((state) => state.initialize);
  const notifiedVersion = useRef<string | null>(null);

  useEffect(() => {
    const timer = window.setTimeout(() => void initialize(), AUTOMATIC_CHECK_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [initialize]);

  useEffect(() => {
    if (phase !== "ready" || !version || notifiedVersion.current === version) return;
    notifiedVersion.current = version;
    toast.success(t("settings.update.ready", { version }), {
      action: { label: t("settings.update.review"), onClick: onOpenSettings },
      duration: 12_000,
    });
  }, [onOpenSettings, phase, t, version]);

  return null;
}
