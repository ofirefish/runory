import { ArrowDown, ArrowUp, Cable, MoreHorizontal, SquarePen, SquareTerminal, Trash2 } from "lucide-react";
import { useEffect, useRef, useState, type MouseEvent, type PointerEventHandler } from "react";
import { cn } from "../../lib/utils";
import type { SessionState } from "../../types/session";
import type { OsDistribution } from "../../types/domain";
import { JumpHostIndicator } from "./JumpHostIndicator";
import { OsLogo } from "./OsLogo";

type Labels = { edit: string; delete: string; moveUp: string; moveDown: string; more: string; status: string; connect?: string; drag?: string };
const closeMenu = (event: MouseEvent<HTMLButtonElement>) => event.currentTarget.closest("details")?.removeAttribute("open");

export function HostItem({ name, address, labels, active = false, state = "idle", osDistribution, summary, jumpHostLabel, onSelect, onConnect, onEdit, onDelete, onMoveUp, onMoveDown, moveUpDisabled, moveDownDisabled, onDragPointerDown, dragging = false, dragDisabled = false, onCreateTunnel, tunnelLabel }: { name: string; address: string; labels: Labels; active?: boolean; state?: SessionState; osDistribution?: OsDistribution; summary?: string; jumpHostLabel?: string; onSelect: () => void; onConnect: () => void; onEdit: () => void; onDelete: () => void; onMoveUp?: () => void; onMoveDown?: () => void; moveUpDisabled?: boolean; moveDownDisabled?: boolean; onDragPointerDown?: PointerEventHandler<HTMLDivElement>; dragging?: boolean; dragDisabled?: boolean; onCreateTunnel?: () => void; tunnelLabel?: string }) {
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
  return <div className={cn("host-item", active && "active", onDragPointerDown && !dragDisabled && "host-draggable", dragging && "host-dragging")}
    title={onDragPointerDown && !dragDisabled ? labels.drag : undefined}
    onDragStart={(event) => event.preventDefault()}
    onPointerDown={(event) => {
      if (dragDisabled || !(event.target instanceof Element) || event.target.closest("button:not(.host-select),details,a,input,select,textarea")) return;
      onDragPointerDown?.(event);
    }}>
    <button type="button" className="host-select" aria-pressed={active} onClick={onSelect} onDoubleClick={onConnect} onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); onConnect(); } }}>{osDistribution ? <OsLogo distribution={osDistribution} state={state} statusLabel={labels.status} /> : <span className={cn("host-status-dot", `status-${state}`)} aria-label={labels.status} title={labels.status} />}<span className="host-copy"><span className="host-name-row"><span className="host-name" title={name}>{name}</span>{jumpHostLabel && <JumpHostIndicator label={jumpHostLabel} />}</span><span className="host-address" title={address}>{address}</span></span></button>
    {summary && <div className="server-host-meta"><span className={cn("server-host-state", `status-${state}`)}>{labels.status}</span><span className="server-host-auth">{summary}</span>{labels.connect && <button type="button" className="server-host-connect" aria-label={`${labels.connect}: ${name}`} title={labels.connect} onClick={onConnect}><SquareTerminal size={15} aria-hidden="true" /></button>}</div>}
    <details ref={menu} className="item-menu host-menu" onToggle={(event) => setMenuOpen(event.currentTarget.open)}><summary aria-label={labels.more} title={labels.more}><MoreHorizontal size={16} /></summary><div className="item-menu-popover" role="menu">
      <button type="button" role="menuitem" onClick={(event) => { closeMenu(event); onEdit(); }}><SquarePen size={14} />{labels.edit}</button>
      {onCreateTunnel && <button type="button" role="menuitem" onClick={(event) => { closeMenu(event); onCreateTunnel(); }}><Cable size={14} aria-hidden="true" />{tunnelLabel}</button>}
      {onMoveUp && <button type="button" role="menuitem" disabled={moveUpDisabled} onClick={(event) => { closeMenu(event); onMoveUp(); }}><ArrowUp size={14} />{labels.moveUp}</button>}
      {onMoveDown && <button type="button" role="menuitem" disabled={moveDownDisabled} onClick={(event) => { closeMenu(event); onMoveDown(); }}><ArrowDown size={14} />{labels.moveDown}</button>}
      <button type="button" role="menuitem" className="danger" onClick={(event) => { closeMenu(event); onDelete(); }}><Trash2 size={14} />{labels.delete}</button>
    </div></details>
  </div>;
}
