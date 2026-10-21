import { useId, type ButtonHTMLAttributes } from "react";
import { cn } from "../../lib/utils";

type SwitchProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onChange" | "role"> & {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label?: string;
};

export function Switch({
  checked,
  onCheckedChange,
  label,
  className,
  disabled,
  id: idProp,
  ...props
}: SwitchProps) {
  const generatedId = useId();
  const id = idProp ?? generatedId;

  return (
    <div className={cn("flex items-center justify-between gap-3", className)}>
      {label ? (
        <label htmlFor={id} className="cursor-pointer text-sm font-medium leading-none text-foreground peer-disabled:cursor-not-allowed peer-disabled:opacity-70">
          {label}
        </label>
      ) : null}
      <button
        id={id}
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        data-state={checked ? "checked" : "unchecked"}
        className={cn(
          "peer inline-flex h-5 w-9 shrink-0 items-center rounded-full border border-transparent transition-colors",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--primary)/0.35)]",
          "disabled:cursor-not-allowed disabled:opacity-50",
          "data-[state=checked]:bg-[hsl(var(--primary))] data-[state=unchecked]:bg-[hsl(var(--border))]",
        )}
        onClick={() => onCheckedChange(!checked)}
        {...props}
      >
        <span
          aria-hidden
          data-state={checked ? "checked" : "unchecked"}
          className={cn(
            "pointer-events-none block size-4 rounded-full bg-white shadow-sm transition-transform",
            "data-[state=checked]:translate-x-4 data-[state=unchecked]:translate-x-0.5",
          )}
        />
      </button>
    </div>
  );
}
