import * as React from "react";
import { cn } from "../../lib/utils";
export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(({ className, ...props }, ref) => <input ref={ref} className={cn("h-9 w-full rounded-md border border-border bg-surface px-3 text-sm text-foreground outline-none transition-colors placeholder:text-[hsl(var(--muted))] focus-visible:ring-2 focus-visible:ring-primary disabled:cursor-not-allowed disabled:opacity-50 aria-[invalid=true]:border-red-500 aria-[invalid=true]:focus-visible:ring-red-500/30", className)} {...props} />);
Input.displayName = "Input";
