import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent, type RefObject } from "react";
import { useCatalogStore } from "../../stores/catalog-store";

type MoveNotice = { name: string };

// Pointer dragging leaves Tauri's native file-drop handler available to SFTP.
export function useProfileGroupDrag(root: RefObject<HTMLElement | null>) {
  const [draggedId, setDraggedId] = useState<string | null>(null);
  const [overGroup, setOverGroup] = useState<string | null>(null);
  const [moving, setMoving] = useState(false);
  const [notice, setNotice] = useState<MoveNotice | null>(null);
  const pending = useRef(false);
  const draggedGesture = useRef(false);
  const cleanup = useRef<() => void>(() => undefined);
  useEffect(() => () => cleanup.current(), []);

  const targetAt = (x: number, y: number) => {
    const target = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-server-group]");
    return target && root.current?.contains(target) ? target.dataset.serverGroup ?? null : null;
  };
  const moveToGroup = async (profileId: string, target: string) => {
    if (pending.current) return;
    const { profiles, groups, updateProfile } = useCatalogStore.getState();
    const profile = profiles.find((item) => item.id === profileId);
    const groupId = target || null;
    const group = groups.find((item) => item.id === groupId);
    // Resolve current metadata at drop time; never submit a stale drag-start profile.
    if (!profile || profile.groupId === groupId || (groupId !== null && !group)) return;
    pending.current = true;
    setMoving(true);
    try {
      // Rust appends a cross-group move under its catalog write lock.
      await updateProfile({ id: profile.id, name: profile.name, host: profile.host, port: profile.port, username: profile.username,
        authMethod: profile.authMethod, keySource: profile.keySource, connectionRoute: profile.connectionRoute, groupId, sortOrder: profile.sortOrder });
    } catch {
      setNotice({ name: profile.name });
    } finally {
      pending.current = false;
      setMoving(false);
    }
  };

  const startDrag = (event: ReactPointerEvent<HTMLDivElement>, profileId: string) => {
    if (event.button !== 0 || !event.isPrimary || pending.current) return;
    cleanup.current();
    const { clientX: startX, clientY: startY, pointerId } = event;
    let started = false;
    const finish = () => {
      cleanup.current();
      setDraggedId(null);
      setOverGroup(null);
    };
    const move = (next: PointerEvent) => {
      if (next.pointerId !== pointerId) return;
      if (!started && Math.hypot(next.clientX - startX, next.clientY - startY) < 6) return;
      started = true;
      draggedGesture.current = true;
      next.preventDefault();
      setNotice(null);
      setDraggedId(profileId);
      setOverGroup(targetAt(next.clientX, next.clientY));
    };
    const drop = (next: PointerEvent) => {
      if (next.pointerId !== pointerId) return;
      const target = started ? targetAt(next.clientX, next.clientY) : null;
      finish();
      if (target !== null) void moveToGroup(profileId, target);
    };
    const cancel = (next: PointerEvent) => { if (next.pointerId === pointerId) finish(); };
    const key = (next: KeyboardEvent) => { if (next.key === "Escape") { next.preventDefault(); finish(); } };
    window.addEventListener("pointermove", move, { passive: false });
    window.addEventListener("pointerup", drop);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("keydown", key);
    window.addEventListener("blur", finish);
    cleanup.current = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", drop);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("keydown", key);
      window.removeEventListener("blur", finish);
    };
  };
  // A pointer drag may synthesize a click on the source or drop target after release.
  // Keep suppressing it until a new pointer gesture; keyboard activation is unaffected.
  const onClickCapture = (event: ReactMouseEvent<HTMLElement>) => {
    if (draggedGesture.current && event.detail > 0) { event.preventDefault(); event.stopPropagation(); }
  };
  const onPointerDownCapture = () => { draggedGesture.current = false; };
  return { draggedId, overGroup, moving, notice, startDrag, onClickCapture, onPointerDownCapture };
}
