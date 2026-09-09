import { X } from "lucide-react";
import { useEffect, useId, useRef, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";

const focusableSelector = [
  "button:not([disabled])",
  "[href]",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

export function SettingsEditorSheet({ title, description, closeDisabled = false, onClose, children }: {
  title: string;
  description?: string;
  closeDisabled?: boolean;
  onClose: () => void;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const titleId = useId();
  const descriptionId = useId();
  const panelRef = useRef<HTMLElement>(null);
  const onCloseRef = useRef(onClose);
  const closeDisabledRef = useRef(closeDisabled);
  onCloseRef.current = onClose;
  closeDisabledRef.current = closeDisabled;

  useEffect(() => {
    const previouslyFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const panel = panelRef.current;
    const focusTarget = panel?.querySelector<HTMLElement>(".settings-editor-sheet-body")?.querySelector<HTMLElement>(focusableSelector)
      ?? panel?.querySelector<HTMLElement>(focusableSelector)
      ?? panel;
    queueMicrotask(() => focusTarget?.focus());

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        if (!closeDisabledRef.current) onCloseRef.current();
        event.preventDefault();
        event.stopPropagation();
        return;
      }
      if (event.key !== "Tab" || !panel) return;
      const focusable = Array.from(panel.querySelectorAll<HTMLElement>(focusableSelector));
      if (focusable.length === 0) {
        event.preventDefault();
        panel.focus();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown, true);
    return () => {
      document.removeEventListener("keydown", handleKeyDown, true);
      queueMicrotask(() => previouslyFocused?.focus());
    };
  }, []);

  return <div className="settings-editor-sheet-layer">
    <button type="button" className="settings-editor-sheet-backdrop" aria-label={t("a11y.close")} disabled={closeDisabled} onClick={onClose} />
    <aside ref={panelRef} className="settings-editor-sheet" role="dialog" aria-modal="true" aria-labelledby={titleId} aria-describedby={description ? descriptionId : undefined} tabIndex={-1}>
      <header className="settings-editor-sheet-header">
        <div><h3 id={titleId}>{title}</h3>{description && <p id={descriptionId}>{description}</p>}</div>
        <Button type="button" variant="ghost" size="icon" disabled={closeDisabled} aria-label={t("a11y.close")} onClick={onClose}><X size={18} /></Button>
      </header>
      <div className="settings-editor-sheet-body">{children}</div>
    </aside>
  </div>;
}
