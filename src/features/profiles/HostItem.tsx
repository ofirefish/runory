import { ArrowDown, ArrowUp, MoreHorizontal, SquarePen, Trash2 } from "lucide-react";
import { useEffect, useRef, useState, type MouseEvent } from "react";
import { cn } from "../../lib/utils";
import type { SessionState } from "../../types/session";
import type { OsDistribution } from "../../types/domain";
import { OsLogo } from "./OsLogo";

type Labels = { edit: string; delete: string; moveUp: string; moveDown: string; more: string; status: string };
const closeMenu = (event: MouseEvent<HTMLButtonElement>) => event.currentTarget.closest("details")?.removeAttribute("open");

export function HostItem({ name, address, labels, active = false, state = "idle", osDistribution, onSelect, onConnect, onEdit, onDelete, onMoveUp, onMoveDown, moveUpDisabled, moveDownDisabled }: { name: string; address: string; labels: Labels; active?: boolean; state?: SessionState; osDistribution?: OsDistribution; onSelect: () => void; onConnect: () => void; onEdit: () => void; onDelete: () => void; onMoveUp?: () => void; onMoveDown?: () => void; moveUpDisabled?: boolean; moveDownDisabled?: boolean }) {
  const menu = useRef<HTMLDetailsElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  useEffect(() => {
    if (!menuOpen) return;
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!menu.current?.contains(event.target as Node)) menu.current?.removeAttribute("open");
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [menuOpen]);
  return <div className={cn("host-item", active && "active")}>
    <button type="button" className="host-select" onClick={onSelect} onDoubleClick={onConnect}>{osDistribution ? <OsLogo distribution={osDistribution} state={state} statusLabel={labels.status} /> : <span className={cn("host-status-dot", `status-${state}`)} aria-label={labels.status} title={labels.status} />}<span className="host-copy"><span className="host-name">{name}</span><span className="host-address">{address}</span></span></button>
    <details ref={menu} className="item-menu host-menu" onToggle={(event) => setMenuOpen(event.currentTarget.open)}><summary aria-label={labels.more} title={labels.more}><MoreHorizontal size={16} /></summary><div className="item-menu-popover" role="menu">
      <button type="button" role="menuitem" onClick={(event) => { closeMenu(event); onEdit(); }}><SquarePen size={14} />{labels.edit}</button>
      {onMoveUp && <button type="button" role="menuitem" disabled={moveUpDisabled} onClick={(event) => { closeMenu(event); onMoveUp(); }}><ArrowUp size={14} />{labels.moveUp}</button>}
      {onMoveDown && <button type="button" role="menuitem" disabled={moveDownDisabled} onClick={(event) => { closeMenu(event); onMoveDown(); }}><ArrowDown size={14} />{labels.moveDown}</button>}
      <button type="button" role="menuitem" className="danger" onClick={(event) => { closeMenu(event); onDelete(); }}><Trash2 size={14} />{labels.delete}</button>
    </div></details>
  </div>;
}
