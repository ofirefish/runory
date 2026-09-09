import * as React from "react";
import { cn } from "@/lib/utils";

export function Badge({ className, variant = "default", ...props }: React.ComponentProps<"span"> & { variant?: "default" | "secondary" | "success" | "warning" | "outline" }) {
  return <span className={cn("inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-medium", variant === "default" && "bg-primary/15 text-indigo-300", variant === "secondary" && "bg-muted text-muted-foreground", variant === "success" && "bg-emerald-400/10 text-emerald-300", variant === "warning" && "bg-amber-400/10 text-amber-300", variant === "outline" && "border border-border text-muted-foreground", className)} {...props} />;
}
