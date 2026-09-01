import { useEffect, useRef, type KeyboardEvent, type ReactNode } from "react";
import { cn } from "../../lib/utils";

export type ContextMenuItem = {
  label: string;
  icon: ReactNode;
  disabled?: boolean;
  destructive?: boolean;
  onSelect: () => void;
};

export function ContextMenu({ x, y, label, items, onClose }: { x: number; y: number; label: string; items: ContextMenuItem[]; onClose: () => void }) {
  const menuRef = useRef<HTMLDivElement>(null);
  const left = Math.max(8, Math.min(x, window.innerWidth - 192));
  const top = Math.max(8, Math.min(y, window.innerHeight - (items.length * 36 + 16)));

  useEffect(() => {
    const close = () => onClose();
    const closeOnEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    const closeOnScroll = () => close();
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", closeOnEscape);
    window.addEventListener("resize", close);
    window.addEventListener("scroll", closeOnScroll, true);
    menuRef.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
    return () => {
      window.removeEventListener("pointerdown", close);
      window.removeEventListener("keydown", closeOnEscape);
      window.removeEventListener("resize", close);
      window.removeEventListener("scroll", closeOnScroll, true);
    };
  }, [onClose]);

  const moveFocus = (event: KeyboardEvent<HTMLDivElement>, direction: 1 | -1) => {
    const buttons = [...(menuRef.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
    if (!buttons.length) return;
    const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const next = current < 0 ? 0 : (current + direction + buttons.length) % buttons.length;
    event.preventDefault();
    buttons[next]?.focus();
  };

  return <div
    ref={menuRef}
    role="menu"
    aria-label={label}
    className="fixed z-50 w-44 rounded-md border border-[hsl(var(--border))] bg-[hsl(var(--surface))] p-1 shadow-lg"
    style={{ left, top }}
    onContextMenu={(event) => event.preventDefault()}
    onKeyDown={(event) => {
      if (event.key === "ArrowDown") moveFocus(event, 1);
      else if (event.key === "ArrowUp") moveFocus(event, -1);
    }}
    onPointerDown={(event) => event.stopPropagation()}
  >
    {items.map((item) => <button
      key={item.label}
      type="button"
      role="menuitem"
      disabled={item.disabled}
      className={cn(
        "flex h-9 w-full items-center gap-2 rounded px-2.5 text-left text-sm outline-none hover:bg-[hsl(var(--elevated))] focus-visible:bg-[hsl(var(--elevated))] disabled:cursor-not-allowed disabled:opacity-40",
        item.destructive && "text-red-500",
      )}
      onClick={() => {
        onClose();
        item.onSelect();
      }}
    >
      {item.icon}
      <span>{item.label}</span>
    </button>)}
  </div>;
}
