import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../lib/utils";

const variants = cva("inline-flex items-center justify-center gap-2 rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--primary))] disabled:pointer-events-none disabled:opacity-50", { variants: { variant: { default: "bg-[hsl(var(--primary))] text-white hover:bg-[hsl(var(--primary)/.9)]", secondary: "bg-[hsl(var(--elevated))] text-[hsl(var(--foreground))] hover:bg-[hsl(var(--border))]", ghost: "hover:bg-[hsl(var(--elevated))]", danger: "bg-red-600 text-white hover:bg-red-500" }, size: { default: "h-9 px-4", icon: "h-9 w-9", sm: "h-8 px-3" } }, defaultVariants: { variant: "default", size: "default" } });
export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof variants> {}
export function Button({ className, variant, size, ...props }: ButtonProps) { return <button className={cn(variants({ variant, size }), className)} {...props} />; }
