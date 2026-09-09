import { useId, type HTMLInputTypeAttribute } from "react";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";
import { Textarea } from "../../components/ui/textarea";

interface FieldProps<T extends string = string> {
  label: string;
  value: T;
  onChange: (value: T) => void;
}

export function DeploymentInputField({ label, value, onChange, type = "text", placeholder }: FieldProps & { type?: HTMLInputTypeAttribute; placeholder?: string }) {
  const id = useId();
  return <div className="grid min-w-0 gap-1.5">
    <Label htmlFor={id}>{label}</Label>
    <Input id={id} type={type} value={value} placeholder={placeholder} onChange={(event) => onChange(event.target.value)} className="text-foreground focus:ring-primary" />
  </div>;
}

export function DeploymentSelectField<T extends string>({ label, value, onChange, options }: FieldProps<T> & { options: readonly { value: T; label: string }[] }) {
  const id = useId();
  return <div className="grid min-w-0 gap-1.5">
    <Label htmlFor={id}>{label}</Label>
    <Select value={value} onValueChange={(next) => {
      const option = options.find((item) => item.value === next);
      if (option) onChange(option.value);
    }}>
      <SelectTrigger id={id}><SelectValue /></SelectTrigger>
      <SelectContent>{options.map((option) => <SelectItem key={option.value} value={option.value}>{option.label}</SelectItem>)}</SelectContent>
    </Select>
  </div>;
}

export function DeploymentTextareaField({ label, value, onChange, description }: FieldProps & { description: string }) {
  const id = useId();
  const descriptionId = `${id}-description`;
  return <div className="grid min-w-0 gap-1.5">
    <Label htmlFor={id}>{label}</Label>
    <Textarea id={id} aria-describedby={descriptionId} className="min-h-56 resize-y font-mono" value={value} onChange={(event) => onChange(event.target.value)} spellCheck={false} />
    <p id={descriptionId} className="text-xs text-[hsl(var(--muted))]">{description}</p>
  </div>;
}
