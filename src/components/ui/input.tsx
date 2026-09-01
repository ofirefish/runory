import * as React from "react";
import { cn } from "../../lib/utils";
export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(({ className, ...props }, ref) => <input ref={ref} className={cn("h-9 w-full rounded-md border border-[hsl(var(--border))] bg-[hsl(var(--surface))] px-3 text-sm outline-none placeholder:text-[hsl(var(--muted))] focus:ring-2 focus:ring-blue-500", className)} {...props} />);
Input.displayName = "Input";
