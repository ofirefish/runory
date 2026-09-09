import * as React from "react";
import { cn } from "@/lib/utils";

export function Table({ className, ...props }: React.ComponentProps<"table">) { return <div className="w-full overflow-x-auto"><table className={cn("w-full caption-bottom text-sm", className)} {...props} /></div>; }
export function TableHeader(props: React.ComponentProps<"thead">) { return <thead className="border-b border-border text-left text-xs uppercase tracking-wider text-muted-foreground" {...props} />; }
export function TableBody(props: React.ComponentProps<"tbody">) { return <tbody className="divide-y divide-border" {...props} />; }
export function TableRow({ className, ...props }: React.ComponentProps<"tr">) { return <tr className={cn("transition-colors hover:bg-muted/45", className)} {...props} />; }
export function TableHead({ className, ...props }: React.ComponentProps<"th">) { return <th className={cn("h-11 px-4 font-medium", className)} {...props} />; }
export function TableCell({ className, ...props }: React.ComponentProps<"td">) { return <td className={cn("px-4 py-4 align-middle", className)} {...props} />; }

