import { Check, ChevronsUpDown } from "lucide-react";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { Command, CommandEmpty, CommandInput, CommandItem, CommandList } from "../../components/ui/command";
import { Popover, PopoverAnchor, PopoverContent, PopoverTrigger } from "../../components/ui/popover";

export function ModelNameInput({ id, value, onChange, models, disabled }: { id: string; value: string; onChange: (value: string) => void; models: readonly string[]; disabled: boolean }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  return <Popover open={open && !disabled} onOpenChange={setOpen}>
    <PopoverAnchor asChild><div className="flex min-w-0 gap-2">
      <Input id={id} ref={input} required disabled={disabled} value={value} onChange={(event) => onChange(event.target.value)} placeholder={t("settings.modelName")} />
      <PopoverTrigger asChild><Button type="button" variant="secondary" size="icon" className="h-[38px] shrink-0" disabled={disabled || models.length === 0} aria-label={t("settings.models.suggestions")}><ChevronsUpDown size={16} /></Button></PopoverTrigger>
    </div></PopoverAnchor>
    <PopoverContent align="start" className="w-[var(--radix-popover-trigger-width)] min-w-64 p-0" onCloseAutoFocus={(event) => { event.preventDefault(); input.current?.focus(); }}>
      <Command label={t("settings.models.suggestions")}>
        <CommandInput placeholder={t("settings.modelName")} aria-label={t("settings.models.suggestions")} />
        <CommandList label={t("settings.models.suggestions")}><CommandEmpty>{t("settings.models.noSuggestions")}</CommandEmpty>{models.map((model) => <CommandItem key={model} value={model} onSelect={() => { onChange(model); setOpen(false); }}><Check size={14} className={model === value ? "opacity-100" : "opacity-0"} aria-hidden="true" />{model}</CommandItem>)}</CommandList>
      </Command>
    </PopoverContent>
  </Popover>;
}
