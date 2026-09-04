import * as React from "react";
import { cn } from "../../lib/utils";

const Textarea = React.forwardRef<HTMLTextAreaElement, React.ComponentPropsWithoutRef<"textarea">>(
  ({ className, ...props }, ref) => (
    <textarea
      ref={ref}
      data-slot="textarea"
      className={cn("flex min-h-[60px] w-full rounded-md border border-border bg-surface px-3 py-2 text-sm text-foreground placeholder:text-[hsl(var(--muted))] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary disabled:cursor-not-allowed disabled:opacity-50", className)}
      {...props}
    />
  ),
);
Textarea.displayName = "Textarea";

export { Textarea };
