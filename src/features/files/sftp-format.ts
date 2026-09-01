export function breadcrumbPaths(path: string): Array<{ label: string; path: string }> {
  const parts = path.split("/").filter(Boolean);
  return [
    { label: "/", path: "/" },
    ...parts.map((label, index) => ({ label, path: `/${parts.slice(0, index + 1).join("/")}` })),
  ];
}

export function formatPermissions(value: number | null): string {
  return value === null ? "—" : (value & 0o7777).toString(8).padStart(4, "0");
}

export function formatFileSize(value: number | null, locale: string): string {
  if (value === null) return "—";
  if (value < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let size = value / 1024;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: size < 10 ? 1 : 0 }).format(size)} ${units[unit]}`;
}
