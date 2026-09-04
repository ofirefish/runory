import { useId } from "react";
import { Label } from "../../components/ui/label";
import { SelectControl, type SelectControlProps } from "../../components/ui/select-control";

export function SettingsSelectField<T extends string>({ className, controlClassName, ...props }: Omit<SelectControlProps<T>, "id"> & { controlClassName?: string }) {
  const id = useId();
  return <div className={className ?? "grid min-w-0 gap-2"}>
    <Label htmlFor={id} className="text-xs">{props.label}</Label>
    <SelectControl {...props} id={id} className={controlClassName} />
  </div>;
}
