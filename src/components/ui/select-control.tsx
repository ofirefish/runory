import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./select";

export interface SelectControlProps<T extends string> {
  value: T;
  onValueChange: (value: T) => void;
  options: readonly { value: T; label: string }[];
  label: string;
  id?: string;
  disabled?: boolean;
  className?: string;
}

export function SelectControl<T extends string>({ value, onValueChange, options, label, id, disabled, className }: SelectControlProps<T>) {
  return <Select value={value} disabled={disabled} onValueChange={(next) => {
    const option = options.find((item) => item.value === next);
    if (option) onValueChange(option.value);
  }}>
    <SelectTrigger id={id} aria-label={label} className={className}><SelectValue /></SelectTrigger>
    <SelectContent>{options.map((option) => <SelectItem key={option.value} value={option.value}>{option.label}</SelectItem>)}</SelectContent>
  </Select>;
}
