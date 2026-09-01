import { useTranslation } from "react-i18next";
import { MOBILE_EXTRA_KEYS } from "./mobile-extra-keys";

export function MobileExtraKeys({ disabled, onInput }: { disabled: boolean; onInput: (value: string) => void }) {
  const { t } = useTranslation();
  return <div className="terminal-extra-keys flex h-11 shrink-0 gap-1 overflow-x-auto px-1.5 py-1 md:hidden" aria-label={t("mobile.extraKeys")}>
    {MOBILE_EXTRA_KEYS.map(([label, value]) => <button key={label} type="button" disabled={disabled} className="min-w-11 shrink-0 rounded px-2 font-mono text-xs disabled:opacity-40" aria-label={t("mobile.sendKey", { key: label })} onPointerDown={(event) => event.preventDefault()} onClick={() => onInput(value)}>{label}</button>)}
  </div>;
}
