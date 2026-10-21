import { Puzzle } from "lucide-react";
import { cn } from "../../../lib/utils";

const SIZE_MAP = {
  sm: 24,
  md: 32,
  lg: 48,
  xl: 64,
} as const;

export function PluginLogo({
  title,
  logoUrl,
  size = "md",
  className,
}: {
  title: string;
  logoUrl?: string | null;
  size?: keyof typeof SIZE_MAP;
  className?: string;
}) {
  const px = SIZE_MAP[size];
  if (logoUrl) {
    return (
      <img
        src={logoUrl}
        alt=""
        width={px}
        height={px}
        className={cn("shrink-0 rounded object-contain", className)}
        style={{ width: px, height: px }}
      />
    );
  }
  return (
    <span
      className={cn(
        "grid shrink-0 place-items-center rounded bg-[hsl(var(--elevated))] text-[hsl(var(--secondary))]",
        className,
      )}
      style={{ width: px, height: px }}
      aria-hidden="true"
      title={title}
    >
      <Puzzle size={Math.max(12, Math.floor(px * 0.5))} />
    </span>
  );
}
