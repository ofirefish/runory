import { Send, Square } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { parseFleetMentions } from "../../../lib/agent/fleet-mentions";
import type { FleetExecutionStrategyV2 } from "../../../types/agent-v2";

export type AgentComposerMentionOption = {
  id: string;
  kind: "server" | "group";
  label: string;
  insertText: string;
  available: boolean;
};

export type AgentComposerSubmitOptions = {
  fleetStrategy: FleetExecutionStrategyV2;
};

/**
 * Fixed composer at the panel bottom. Enter sends, Shift+Enter inserts a
 * newline. The mention picker only inserts user-facing names. Exact profile
 * and session authorization remains a Rust preflight concern.
 */
export function AgentComposer({ onSubmit, onCancel, disabled, running = false, placeholder, mentionOptions = [] }: {
  onSubmit: (text: string, options: AgentComposerSubmitOptions) => void;
  onCancel?: () => void;
  disabled?: boolean;
  running?: boolean;
  placeholder?: string;
  mentionOptions?: AgentComposerMentionOption[];
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState("");
  const [activeOption, setActiveOption] = useState(0);
  const [fleetStrategy, setFleetStrategy] = useState<FleetExecutionStrategyV2>("sequential");
  const ref = useRef<HTMLTextAreaElement>(null);
  const mentionQuery = activeMentionQuery(draft, ref.current?.selectionStart ?? draft.length);
  const suggestions = useMemo(() => {
    if (!mentionQuery) return [];
    const normalized = mentionQuery.query.toLocaleLowerCase();
    return mentionOptions
      .filter((option) => mentionQuery.kind === null || option.kind === mentionQuery.kind)
      .filter((option) => option.label.toLocaleLowerCase().includes(normalized))
      .slice(0, 8);
  }, [mentionOptions, mentionQuery]);
  const chips = parseFleetMentions(draft).mentions;

  useEffect(() => {
    if (!disabled) ref.current?.focus();
  }, [disabled]);

  const send = () => {
    const text = draft.trim();
    if (!text || disabled || running) return;
    onSubmit(text, { fleetStrategy });
    setDraft("");
  };
  const onKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (suggestions.length > 0 && event.key === "ArrowDown") {
      event.preventDefault();
      setActiveOption((current) => (current + 1) % suggestions.length);
      return;
    }
    if (suggestions.length > 0 && event.key === "ArrowUp") {
      event.preventDefault();
      setActiveOption((current) => (current - 1 + suggestions.length) % suggestions.length);
      return;
    }
    if (suggestions.length > 0 && event.key === "Escape") {
      event.preventDefault();
      setActiveOption(-1);
      return;
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (mentionQuery && suggestions.length > 0 && activeOption >= 0) {
        insertMention(suggestions[Math.min(activeOption, suggestions.length - 1)], mentionQuery);
        return;
      }
      send();
    }
  };
  const resize = (element: HTMLTextAreaElement) => {
    element.style.height = "0px";
    element.style.height = `${Math.min(element.scrollHeight, 120)}px`;
  };
  const insertMention = (option: AgentComposerMentionOption, query = mentionQuery) => {
    if (!query) return;
    const next = `${draft.slice(0, query.start)}${option.insertText} ${draft.slice(query.end)}`;
    const caret = query.start + option.insertText.length + 1;
    setDraft(next);
    setActiveOption(0);
    window.requestAnimationFrame(() => {
      ref.current?.focus();
      ref.current?.setSelectionRange(caret, caret);
    });
  };

  return <div className="agent-composer">
    <div className="agent-composer-shell">
      <textarea
        ref={ref}
        value={draft}
        onChange={(event) => { setDraft(event.target.value); setActiveOption(0); resize(event.target); }}
        onKeyDown={onKeyDown}
        rows={1}
        disabled={disabled || running}
        placeholder={placeholder ?? t("contextPanel.composerPlaceholder")}
        aria-label={t("contextPanel.composerLabel")}
        aria-disabled={disabled || running || undefined}
        aria-controls={suggestions.length > 0 ? "agent-mention-options" : undefined}
        aria-expanded={suggestions.length > 0}
        aria-activedescendant={suggestions.length > 0 && activeOption >= 0 ? `agent-mention-${suggestions[activeOption]?.id}` : undefined}
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
    {suggestions.length > 0 && activeOption >= 0 && <div id="agent-mention-options" className="agent-mention-options" role="listbox" aria-label={t("contextPanel.mentions.label")}>
      {suggestions.map((option, index) => <button
        type="button"
        role="option"
        id={`agent-mention-${option.id}`}
        aria-selected={index === activeOption}
        className={index === activeOption ? "active" : undefined}
        key={`${option.kind}-${option.id}`}
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => insertMention(option)}
      >
        <span>{option.label}</span>
        <small>{t(`contextPanel.mentions.${option.kind}`)} · {t(option.available ? "contextPanel.mentions.connected" : "contextPanel.mentions.unavailable")}</small>
      </button>)}
    </div>}
    {chips.length > 0 && <div className="agent-mention-chips" aria-label={t("contextPanel.mentions.selected")}>
      {chips.map((mention) => <span key={`${mention.start}-${mention.raw}`}>{mention.raw}</span>)}
    </div>}
    {chips.length > 0 && <label className="agent-fleet-strategy">
      <span>{t("contextPanel.fleet.strategy")}</span>
      <select value={fleetStrategy} onChange={(event) => setFleetStrategy(event.target.value as FleetExecutionStrategyV2)} disabled={disabled || running}>
        <option value="sequential">{t("contextPanel.fleet.strategy.sequential")}</option>
        <option value="canary">{t("contextPanel.fleet.strategy.canary")}</option>
        <option value="rolling_batch">{t("contextPanel.fleet.strategy.rolling_batch")}</option>
      </select>
    </label>}
  </div>;
}

type ActiveMentionQuery = { start: number; end: number; query: string; kind: "group" | null };

function activeMentionQuery(text: string, caret: number): ActiveMentionQuery | null {
  const before = text.slice(0, caret);
  const start = before.lastIndexOf("@");
  if (start < 0 || (start > 0 && !/[\s,，;；。!?！？()[\]{}<>]/.test(text[start - 1] ?? ""))) return null;
  const token = before.slice(start + 1);
  if (/[\s,，;；。!?！？()[\]{}<>#]/.test(token)) return null;
  const group = token.toLocaleLowerCase().startsWith("group:");
  return { start, end: caret, query: group ? token.slice(6) : token, kind: group ? "group" : null };
}
