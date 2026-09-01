import { Send, Square } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

/**
 * Fixed composer at the panel bottom. Enter sends, Shift+Enter inserts a
 * newline. `@` prefixes are reserved for future context mentions
 * (@server / @terminal / @logs / @file) — the structure supports them,
 * they are not parsed yet.
 */
export function AgentComposer({ onSubmit, onCancel, disabled, running = false, placeholder }: {
  onSubmit: (text: string) => void;
  onCancel?: () => void;
  disabled?: boolean;
  running?: boolean;
  placeholder?: string;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState("");
  const ref = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (!disabled) ref.current?.focus();
  }, [disabled]);

  const send = () => {
    const text = draft.trim();
    if (!text || disabled || running) return;
    onSubmit(text);
    setDraft("");
  };
  const onKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      send();
    }
  };
  const resize = (element: HTMLTextAreaElement) => {
    element.style.height = "0px";
    element.style.height = `${Math.min(element.scrollHeight, 120)}px`;
  };

  return <div className="agent-composer">
    <div className="agent-composer-shell">
      <textarea
        ref={ref}
        value={draft}
        onChange={(event) => { setDraft(event.target.value); resize(event.target); }}
        onKeyDown={onKeyDown}
        rows={1}
        disabled={disabled || running}
        placeholder={placeholder ?? t("contextPanel.composerPlaceholder")}
        aria-label={t("contextPanel.composerLabel")}
        aria-disabled={disabled || running || undefined}
      />
      {running ? (
        <button type="button" className="agent-send agent-stop" disabled={!onCancel} onClick={onCancel} aria-label={t("contextPanel.stop")} title={t("contextPanel.stop")}>
          <Square size={11} fill="currentColor" />
        </button>
      ) : (
        <button type="button" className="agent-send" disabled={disabled || !draft.trim()} onClick={send} aria-label={t("contextPanel.send")} title={t("contextPanel.send")}>
          <Send size={14} />
        </button>
      )}
    </div>
  </div>;
}
