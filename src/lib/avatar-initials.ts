/** Default avatar label when no photo is uploaded. Prefer a single leading character (首字母). */
export function avatarInitials(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) return "?";
  const segments = trimmed.split(/\s+/).filter(Boolean);
  if (segments.length > 1) {
    const first = firstGrapheme(segments[0] ?? "");
    const last = firstGrapheme(segments.at(-1) ?? "");
    return normalizeInitial(`${first}${last}`);
  }
  return normalizeInitial(firstGrapheme(trimmed));
}

function firstGrapheme(value: string): string {
  return [...value][0] ?? "";
}

function normalizeInitial(value: string): string {
  return /[A-Za-z]/.test(value) ? value.toUpperCase() : value;
}
