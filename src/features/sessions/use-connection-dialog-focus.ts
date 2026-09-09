import { useEffect, useRef, type KeyboardEvent } from "react";

export function useConnectionDialogFocus(busy: boolean, onClose: () => void) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const previous = document.activeElement;
    if (!ref.current?.contains(previous)) ref.current?.focus();
    return () => { if (previous instanceof HTMLElement && previous.isConnected) previous.focus(); };
  }, []);

  useEffect(() => {
    if (busy) ref.current?.focus();
  }, [busy]);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      if (!busy) onClose();
    }
    if (event.key !== "Tab") return;
    const controls = [...event.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), [tabindex="0"]')]
      .filter((element) => !element.closest("[hidden]"));
    const first = controls[0];
    const last = controls.at(-1);
    if (!first) { event.preventDefault(); ref.current?.focus(); return; }
    if (event.shiftKey && (document.activeElement === first || document.activeElement === ref.current)) {
      event.preventDefault(); last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault(); first.focus();
    }
  };
  return { ref, onKeyDown };
}
