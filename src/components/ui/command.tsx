import * as React from "react";
import { Command as CommandPrimitive } from "cmdk";
import { Search } from "lucide-react";
import { cn } from "../../lib/utils";

const Command = React.forwardRef<React.ComponentRef<typeof CommandPrimitive>, React.ComponentPropsWithoutRef<typeof CommandPrimitive>>(
  ({ className, ...props }, ref) => <CommandPrimitive ref={ref} className={cn("flex h-full w-full flex-col overflow-hidden rounded-md bg-surface text-foreground", className)} {...props} />,
);
Command.displayName = CommandPrimitive.displayName;
const CommandInput = React.forwardRef<React.ComponentRef<typeof CommandPrimitive.Input>, React.ComponentPropsWithoutRef<typeof CommandPrimitive.Input>>(
  ({ className, ...props }, ref) => <div className="flex items-center border-b px-3">
    <Search className="mr-2 h-4 w-4 shrink-0 text-[hsl(var(--secondary))]" aria-hidden="true" />
    <CommandPrimitive.Input ref={ref} className={cn("flex h-10 w-full rounded-md bg-transparent py-3 text-sm outline-none placeholder:text-[hsl(var(--muted))] disabled:cursor-not-allowed disabled:opacity-50", className)} {...props} />
  </div>,
);
CommandInput.displayName = CommandPrimitive.Input.displayName;
const CommandList = React.forwardRef<React.ComponentRef<typeof CommandPrimitive.List>, React.ComponentPropsWithoutRef<typeof CommandPrimitive.List>>(
  ({ className, ...props }, ref) => <CommandPrimitive.List ref={ref} className={cn("max-h-[300px] overflow-y-auto overflow-x-hidden p-1", className)} {...props} />,
);
CommandList.displayName = CommandPrimitive.List.displayName;
const CommandEmpty = React.forwardRef<React.ComponentRef<typeof CommandPrimitive.Empty>, React.ComponentPropsWithoutRef<typeof CommandPrimitive.Empty>>(
  ({ className, ...props }, ref) => <CommandPrimitive.Empty ref={ref} className={cn("py-6 text-center text-sm", className)} {...props} />,
);
CommandEmpty.displayName = CommandPrimitive.Empty.displayName;
const CommandItem = React.forwardRef<React.ComponentRef<typeof CommandPrimitive.Item>, React.ComponentPropsWithoutRef<typeof CommandPrimitive.Item>>(
  ({ className, ...props }, ref) => <CommandPrimitive.Item ref={ref} className={cn("relative flex cursor-default select-none items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-none data-[disabled=true]:pointer-events-none data-[selected=true]:bg-[hsl(var(--elevated))] data-[selected=true]:text-foreground data-[disabled=true]:opacity-50", className)} {...props} />,
);
CommandItem.displayName = CommandPrimitive.Item.displayName;
export { Command, CommandInput, CommandList, CommandEmpty, CommandItem };
