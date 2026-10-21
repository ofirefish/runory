import type { SearchableComboboxOptionGroup } from "../../components/ui/searchable-combobox";
import type { HostGroup, ServerProfile } from "../../types/domain";
import { z } from "zod";

export function profileComboboxGroups(
  groups: readonly HostGroup[],
  profiles: readonly ServerProfile[],
  ungroupedLabel: string,
): SearchableComboboxOptionGroup<string>[] {
  const result: SearchableComboboxOptionGroup<string>[] = [];
  const sortedGroups = [...groups].sort((left, right) => left.sortOrder - right.sortOrder);
  for (const group of sortedGroups) {
    const items = profiles.filter((profile) => profile.groupId === group.id).sort((left, right) => left.sortOrder - right.sortOrder);
    if (items.length > 0) {
      result.push({
        label: group.name,
        options: items.map((profile) => ({
          value: profile.id,
          label: profile.name,
          keywords: `${profile.name} ${profile.host} ${profile.port} ${profile.username}`,
        })),
      });
    }
  }
  const ungrouped = profiles.filter((profile) => profile.groupId === null).sort((left, right) => left.sortOrder - right.sortOrder);
  if (ungrouped.length > 0) {
    result.push({
      label: ungroupedLabel,
      options: ungrouped.map((profile) => ({
        value: profile.id,
        label: profile.name,
        keywords: `${profile.name} ${profile.host} ${profile.port} ${profile.username}`,
      })),
    });
  }
  return result;
}

export const tunnelSchema = z.object({
  name: z.string().trim().min(1).max(120).refine((value) => !/\p{Cc}/u.test(value)),
  profileId: z.string().min(1),
  targetHost: z.string().trim().min(1).max(253).refine((value) =>
    z.ipv4().safeParse(value).success || z.ipv6().safeParse(value).success ||
    value.replace(/\.$/, "").split(".").every((part) => /^[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?$/.test(part))),
  targetPort: z.number().int().min(1).max(65535),
  localPort: z.number().int().min(1).max(65535),
});
export type TunnelFormValues = z.infer<typeof tunnelSchema>;
