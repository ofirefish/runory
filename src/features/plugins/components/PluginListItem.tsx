import { useTranslation } from "react-i18next";
import { BadgeCheck } from "lucide-react";
import { PluginIcon } from "./PluginIcon";

export type PluginListEntry = {
  id: string;
  name: string;
  publisherId: string;
  version: string;
  description?: string | null;
  verified?: boolean;
  enabled?: boolean;
  installed?: boolean;
};

export function PluginListItem({
  entry,
  selected,
  onSelect,
  actionLabel,
  actionDisabled,
  onAction,
}: {
  entry: PluginListEntry;
  selected: boolean;
  onSelect: () => void;
  actionLabel?: string;
  actionDisabled?: boolean;
  onAction?: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div
      role="option"
      tabIndex={0}
      aria-selected={selected}
      className={`plugin-list-item${selected ? " is-selected" : ""}`}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelect();
        }
      }}
    >
      <PluginIcon id={entry.id} name={entry.name} size="sm" />
      <div className="plugin-list-item-body">
        <div className="plugin-list-item-title-row">
          <span className="plugin-list-item-name">{entry.name}</span>
          {entry.verified ? (
            <BadgeCheck
              size={12}
              className="plugin-list-item-verified"
              aria-label={t("plugins.marketplace.verified")}
            />
          ) : null}
          {entry.enabled === false ? (
            <span className="plugin-list-item-badge">{t("plugins.status.disabled")}</span>
          ) : null}
        </div>
        <div className="plugin-list-item-meta">
          {entry.publisherId}
          <span aria-hidden="true"> · </span>
          v{entry.version}
        </div>
        {entry.description ? <p className="plugin-list-item-desc">{entry.description}</p> : null}
      </div>
      {actionLabel ? (
        <span className="plugin-list-item-action">
          <button
            type="button"
            disabled={actionDisabled}
            onClick={(event) => {
              event.stopPropagation();
              if (!actionDisabled) onAction?.();
            }}
          >
            {actionLabel}
          </button>
        </span>
      ) : null}
    </div>
  );
}
