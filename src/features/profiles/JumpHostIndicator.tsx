import { Route } from "lucide-react";
import { cn } from "../../lib/utils";

export function JumpHostIndicator({ label, size = 13, className }: { label: string; size?: number; className?: string }) {
  return <span className={cn("jump-host-indicator", className)} role="img" aria-label={label} title={label}>
    <Route size={size} strokeWidth={2} aria-hidden="true" />
  </span>;
}
