import * as React from "react";
import { cn } from "@/lib/utils";

export function Input({ className, ...props }: React.ComponentProps<"input">) {
  return <input className={cn("flex h-11 w-full rounded-xl border border-border bg-background/60 px-3.5 text-sm text-foreground outline-none transition placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50", className)} {...props} />;
}

