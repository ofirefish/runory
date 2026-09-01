import { Copy, Pencil, X } from "lucide-react";
import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";

export type WorkspaceTabMenuState = {
  tabId: string;
  x: number;
  y: number;
};

export function WorkspaceTabMenu({ menu, onClose, onCopy, onRename, onCloseTab }: {
  menu: WorkspaceTabMenuState;
  onClose: () => void;
  onCopy: () => void;
  onRename: () => void;
  onCloseTab: () => void;
}) {
  const { t } = useTranslation();
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const dismissOutside = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) onClose();
    };
    const dismiss = () => onClose();
    const dismissOnEscape = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    document.addEventListener("pointerdown", dismissOutside);
    window.addEventListener("blur", dismiss);
    window.addEventListener("resize", dismiss);
    window.addEventListener("keydown", dismissOnEscape);
    return () => {
      document.removeEventListener("pointerdown", dismissOutside);
      window.removeEventListener("blur", dismiss);
      window.removeEventListener("resize", dismiss);
      window.removeEventListener("keydown", dismissOnEscape);
    };
  }, [onClose]);

  return createPortal(<div ref={menuRef} className="workspace-tab-menu" role="menu" aria-label={t("terminal.tabMenu")} style={{ left: menu.x, top: menu.y }}>
    <button type="button" role="menuitem" autoFocus onClick={onCopy}><Copy size={14} />{t("terminal.copyTab")}</button>
    <button type="button" role="menuitem" onClick={onRename}><Pencil size={14} />{t("terminal.renameTab")}</button>
    <div className="workspace-tab-menu-separator" role="separator" />
    <button type="button" role="menuitem" onClick={onCloseTab}><X size={14} />{t("common.close")}</button>
  </div>, document.body);
}
