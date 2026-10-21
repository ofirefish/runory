const ACCENTS = [
  { bg: "hsl(239 84% 67% / 0.18)", fg: "hsl(239 84% 72%)" },
  { bg: "hsl(199 89% 48% / 0.18)", fg: "hsl(199 89% 65%)" },
  { bg: "hsl(172 66% 40% / 0.18)", fg: "hsl(172 66% 55%)" },
  { bg: "hsl(142 71% 40% / 0.18)", fg: "hsl(142 71% 55%)" },
  { bg: "hsl(280 67% 55% / 0.18)", fg: "hsl(280 67% 70%)" },
  { bg: "hsl(24 95% 53% / 0.18)", fg: "hsl(24 95% 65%)" },
];

function accentFor(id: string) {
  let hash = 0;
  for (let i = 0; i < id.length; i += 1) {
    hash = (hash * 31 + id.charCodeAt(i)) >>> 0;
  }
  return ACCENTS[hash % ACCENTS.length];
}

export function PluginIcon({
  id,
  name,
  size = "md",
}: {
  id: string;
  name: string;
  size?: "sm" | "md" | "lg";
}) {
  const accent = accentFor(id);
  const letter = (name.trim().charAt(0) || id.charAt(0) || "P").toUpperCase();
  return (
    <span
      className={`plugin-icon plugin-icon-${size}`}
      style={{ background: accent.bg, color: accent.fg }}
      aria-hidden="true"
    >
      {letter}
    </span>
  );
}
