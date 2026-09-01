import { ArrowDown, ArrowUp, ChevronDown, MoreHorizontal, Plus, SquarePen, Trash2 } from "lucide-react";
import type { MouseEvent, ReactNode } from "react";

type Labels = { add: string; edit: string; delete: string; moveUp: string; moveDown: string; more: string };
const closeMenu = (event: MouseEvent<HTMLButtonElement>) => event.currentTarget.closest("details")?.removeAttribute("open");

export function GroupSection({ name, count, collapsed = false, system = false, labels, onToggle, onAdd, onEdit, onDelete, onMoveUp, onMoveDown, moveUpDisabled, moveDownDisabled, children }: { name: string; count: number; collapsed?: boolean; system?: boolean; labels: Labels; onToggle?: () => void; onAdd: () => void; onEdit?: () => void; onDelete?: () => void; onMoveUp?: () => void; onMoveDown?: () => void; moveUpDisabled?: boolean; moveDownDisabled?: boolean; children: ReactNode }) {
  return <section className="resource-group">
    <div className="group-header">
      <button type="button" className="group-toggle" onClick={onToggle} disabled={system} aria-expanded={!collapsed}><ChevronDown size={13} className={collapsed ? "-rotate-90" : ""} aria-hidden="true" /><span className="truncate">{name}</span><span className="group-count">{count}</span></button>
      <details className="item-menu group-menu"><summary aria-label={labels.more} title={labels.more}><MoreHorizontal size={15} /></summary><div className="item-menu-popover" role="menu">
        <button type="button" role="menuitem" onClick={(event) => { closeMenu(event); onAdd(); }}><Plus size={14} />{labels.add}</button>
        {onEdit && <button type="button" role="menuitem" onClick={(event) => { closeMenu(event); onEdit(); }}><SquarePen size={14} />{labels.edit}</button>}
        {onMoveUp && <button type="button" role="menuitem" disabled={moveUpDisabled} onClick={(event) => { closeMenu(event); onMoveUp(); }}><ArrowUp size={14} />{labels.moveUp}</button>}
        {onMoveDown && <button type="button" role="menuitem" disabled={moveDownDisabled} onClick={(event) => { closeMenu(event); onMoveDown(); }}><ArrowDown size={14} />{labels.moveDown}</button>}
        {onDelete && <button type="button" role="menuitem" className="danger" onClick={(event) => { closeMenu(event); onDelete(); }}><Trash2 size={14} />{labels.delete}</button>}
      </div></details>
    </div>
    {!collapsed && <div className="group-hosts">{children}</div>}
  </section>;
}
