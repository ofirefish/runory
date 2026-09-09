import { ArrowUpRight, Search } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { OsLogo } from "../../features/profiles/OsLogo";
import { filterProfiles } from "../../features/profiles/search";
import { useCatalogStore } from "../../stores/catalog-store";
import { Popover, PopoverAnchor, PopoverContent } from "../ui/popover";
import "./global-search.css";

export function GlobalSearch({ onOpenProfile }: { onOpenProfile: (profileId: string) => void }) {
  const { t } = useTranslation();
  const profiles = useCatalogStore((state) => state.profiles);
  const groups = useCatalogStore((state) => state.groups);
  const loading = useCatalogStore((state) => state.loading);
  const errorCode = useCatalogStore((state) => state.errorCode);
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const listId = useId();
  const results = query.trim() && !loading && !errorCode ? filterProfiles(groups, profiles, query) : [];
  const selectedIndex = Math.max(0, results.findIndex((profile) => profile.id === selectedId));
  const selected = results[selectedIndex];
  const shortcut = navigator.userAgent.includes("Macintosh") ? "⌘ K" : "Ctrl K";

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey && !event.isComposing && event.key.toLowerCase() === "k") {
        // Do not steal focus from a modal connection or settings flow.
        if (document.querySelector('[role="dialog"][aria-modal="true"]')) return;
        event.preventDefault();
        // Consume the app shortcut before xterm handles it or sends a control key to SSH.
        event.stopPropagation();
        input.current?.focus();
        input.current?.select();
        setOpen(true);
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, []);

  useEffect(() => {
    if (open) list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView?.({ block: "nearest" });
  }, [open, selected?.id]);

  const openProfile = (profileId: string) => {
    setOpen(false);
    setQuery("");
    setSelectedId(null);
    onOpenProfile(profileId);
  };

  return <Popover open={open} onOpenChange={setOpen}>
    <PopoverAnchor asChild>
      <div className="global-search" data-no-drag>
        <Search size={16} aria-hidden="true" />
        <input ref={input} role="combobox" aria-label={t("shell.globalSearch")} aria-expanded={open} aria-controls={open ? listId : undefined} aria-autocomplete="list" aria-activedescendant={open && selected ? `${listId}-${selectedIndex}` : undefined}
          value={query} placeholder={t("shell.globalSearch")} onFocus={() => setOpen(true)} onClick={() => setOpen(true)}
          onChange={(event) => { setQuery(event.target.value); setSelectedId(null); setOpen(true); }}
          onKeyDown={(event) => {
            if (event.nativeEvent.isComposing) return;
            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
              event.preventDefault();
              setOpen(true);
              if (results.length) setSelectedId(results[open ? (selectedIndex + (event.key === "ArrowDown" ? 1 : -1) + results.length) % results.length : selectedIndex].id);
            } else if (event.key === "Enter" && open && selected) {
              event.preventDefault();
              openProfile(selected.id);
            } else if (event.key === "Escape") {
              event.preventDefault();
              setOpen(false);
            }
          }} />
        <kbd>{shortcut}</kbd>
      </div>
    </PopoverAnchor>
    <PopoverContent className="global-search-popover" align="start" sideOffset={8} aria-label={t("shell.searchResults")} data-no-drag
      onOpenAutoFocus={(event) => event.preventDefault()} onCloseAutoFocus={(event) => event.preventDefault()}
      onInteractOutside={(event) => { if (event.target === input.current) event.preventDefault(); }}>
      <div className="global-search-heading">{t("shell.searchResults")}</div>
      <div ref={list} id={listId} role="listbox" aria-label={t("shell.searchResults")} className="global-search-results">
        {results.map((profile, index) => {
          const groupName = groups.find((group) => group.id === profile.groupId)?.name ?? t("sidebar.ungrouped");
          const address = `${profile.username}@${profile.host}:${profile.port}`;
          return <button key={profile.id} id={`${listId}-${index}`} type="button" role="option" aria-selected={index === selectedIndex} tabIndex={-1} className="global-search-result"
            onMouseDown={(event) => event.preventDefault()} onMouseEnter={() => setSelectedId(profile.id)} onClick={() => openProfile(profile.id)}>
            <OsLogo plain distribution={profile.osDistribution ?? "linux"} state="idle" statusLabel={t("status.idle")} />
            <span className="global-search-result-text">
              <span className="global-search-result-title">
                <small title={groupName}>{groupName}</small>
                <i aria-hidden="true">/</i>
                <strong title={profile.name}>{profile.name}</strong>
              </span>
              <span title={address}>{address}</span>
            </span>
            <ArrowUpRight size={14} aria-hidden="true" />
          </button>;
        })}
      </div>
      {!results.length && <p className="global-search-message" role="status">{t(loading ? "common.loading" : errorCode ? "shell.searchUnavailable" : query.trim() ? "shell.searchNoResults" : "shell.searchHint")}</p>}
    </PopoverContent>
  </Popover>;
}
