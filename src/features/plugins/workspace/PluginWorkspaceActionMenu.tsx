import { MoreHorizontal } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import type { PluginWorkspaceEntry } from "./types";

export function PluginWorkspaceActionMenu({
  entry,
  onAction,
}: {
  entry: PluginWorkspaceEntry;
  onAction: (command: string) => void;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  const overflow = useMemo(() => {
    return entry.actions
      .map((command, index) => ({
        command,
        title: entry.actionTitles[index] ?? command,
      }))
      .filter((item) => item.command !== entry.command);
  }, [entry]);

  if (overflow.length === 0) {
    return null;
  }

  return (
    <div className="relative">
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="h-8 w-8"
        aria-label={t("plugins.workspace.moreActions")}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <MoreHorizontal size={16} />
      </Button>
      {open ? (
        <>
          <button
            type="button"
            className="fixed inset-0 z-40 cursor-default"
            aria-label={t("common.close")}
            onClick={() => setOpen(false)}
          />
          <ul
            className="absolute right-0 z-50 mt-1 min-w-40 rounded-md border border-border bg-[hsl(var(--surface))] py-1 shadow-md"
            role="menu"
          >
            {overflow.map((item) => (
              <li key={item.command} role="none">
                <button
                  type="button"
                  role="menuitem"
                  className="flex w-full px-3 py-2 text-left text-sm hover:bg-[hsl(var(--elevated))]"
                  onClick={() => {
                    setOpen(false);
                    onAction(item.command);
                  }}
                >
                  {item.title}
                </button>
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </div>
  );
}
