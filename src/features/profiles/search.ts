import type { HostGroup, ServerProfile } from "../../types/domain";

export function filterProfiles(groups: HostGroup[], profiles: ServerProfile[], query: string): ServerProfile[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return profiles;
  const matchedGroups = new Set(groups.filter((group) => group.name.toLocaleLowerCase().includes(normalized)).map((group) => group.id));
  return profiles.filter((profile) => matchedGroups.has(profile.groupId ?? "") || [profile.name, profile.host, profile.username].some((value) => value.toLocaleLowerCase().includes(normalized)));
}
