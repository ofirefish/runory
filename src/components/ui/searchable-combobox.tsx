import { Check, ChevronsUpDown } from "lucide-react";
import { useState } from "react";
import { cn } from "../../lib/utils";
import { Button } from "./button";
import { Command, CommandEmpty, CommandInput, CommandItem, CommandList } from "./command";
import { Popover, PopoverContent, PopoverTrigger } from "./popover";

export type SearchableComboboxOption<T extends string> = {
  value: T;
  label: string;
  keywords?: string;
};

export function SearchableCombobox<T extends string>({
  id,
  label,
  value,
  options,
  placeholder,
  searchPlaceholder,
  emptyMessage,
  disabled = false,
  invalid = false,
  describedBy,
  onValueChange,
}: {
  id: string;
  label: string;
  value: T;
  options: readonly SearchableComboboxOption<T>[];
  placeholder: string;
  searchPlaceholder: string;
  emptyMessage: string;
  disabled?: boolean;
  invalid?: boolean;
  describedBy?: string;
  onValueChange: (value: T) => void;
}) {
  const [open, setOpen] = useState(false);
  const selected = options.find((option) => option.value === value);

  return <Popover open={open && !disabled} onOpenChange={setOpen}>
    <PopoverTrigger asChild>
      <Button
        id={id}
        type="button"
        variant="ghost"
        role="combobox"
        aria-label={label}
        aria-expanded={open}
        aria-invalid={invalid}
        aria-describedby={describedBy}
        disabled={disabled}
        className={cn(
          "h-9 w-full justify-between border border-border bg-surface px-3 font-normal hover:bg-[hsl(var(--elevated)/.55)]",
          !selected && "text-[hsl(var(--muted))]",
          invalid && "border-red-500 focus-visible:ring-red-500/30",
        )}
      >
        <span className="min-w-0 truncate">{selected?.label ?? placeholder}</span>
        <ChevronsUpDown className="h-4 w-4 shrink-0 text-[hsl(var(--secondary))]" aria-hidden="true" />
      </Button>
    </PopoverTrigger>
    <PopoverContent align="start" className="w-[var(--radix-popover-trigger-width)] min-w-56 p-0">
      <Command label={label}>
        <CommandInput placeholder={searchPlaceholder} aria-label={searchPlaceholder} />
        <CommandList>
          <CommandEmpty>{emptyMessage}</CommandEmpty>
          {options.map((option) => <CommandItem
            key={`${id}:${option.value || "empty"}`}
            value={`${option.label} ${option.keywords ?? ""}`}
            onSelect={() => {
              onValueChange(option.value);
              setOpen(false);
            }}
          >
            <Check className={cn("h-4 w-4 shrink-0", option.value === value ? "opacity-100" : "opacity-0")} aria-hidden="true" />
            <span className="truncate">{option.label}</span>
          </CommandItem>)}
        </CommandList>
      </Command>
    </PopoverContent>
  </Popover>;
}
