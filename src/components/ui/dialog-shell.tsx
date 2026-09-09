import { X } from "lucide-react";
import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { Button } from "./button";

export function DialogShell({ title, onClose, children, size = "default", contentClassName = "", panelClassName = "", closeDisabled = false }: { title: string; onClose: () => void; children: ReactNode; size?: "default" | "form" | "wide" | "pricing"; contentClassName?: string; panelClassName?: string; closeDisabled?: boolean }) {
  const { t } = useTranslation();
  const width = size === "pricing" ? "sm:max-w-6xl" : size === "wide" ? "sm:max-w-4xl" : size === "form" ? "sm:max-w-xl" : "sm:max-w-lg";
  return createPortal(<div className="fixed inset-0 z-50 grid items-end bg-slate-950/60 sm:place-items-center sm:p-4" role="dialog" aria-modal="true" aria-label={title}><div className={`max-h-[92dvh] w-full overflow-y-auto rounded-t-2xl border bg-[hsl(var(--surface))] p-5 pb-[calc(1.25rem+env(safe-area-inset-bottom))] shadow-2xl sm:rounded-xl ${width} ${panelClassName}`}><div className="mb-5 flex items-center justify-between"><h2 className="text-lg font-semibold">{title}</h2><Button type="button" variant="ghost" size="icon" disabled={closeDisabled} aria-label={t("a11y.close")} onClick={onClose}><X size={18} /></Button></div><div className={contentClassName}>{children}</div></div></div>, document.body);
}
