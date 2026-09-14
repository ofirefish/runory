import type { HostGroup, ServerProfile } from "../../../types/domain";
import type { SessionTab } from "../../../stores/session-store";
import type { AgentComposerMentionOption } from "./AgentComposer";

export function buildAgentMentionOptions(
  profiles: ServerProfile[],
  groups: HostGroup[],
  tabs: SessionTab[],
): AgentComposerMentionOption[] {
  const connectedCount = (profileId: string) => tabs.filter((tab) =>
    tab.profileId === profileId && tab.sessionId !== null && tab.state === "connected",
  ).length;
  const quote = (name: string) => /[\s,，;；。!?！？()[\]{}<>#]/.test(name)
    ? `"${name.replaceAll('"', "")}"`
    : name;
  const servers = profiles.map((profile) => ({
    id: profile.id,
    kind: "server" as const,
    label: profile.name,
    insertText: `@${quote(profile.name)}`,
    available: connectedCount(profile.id) === 1,
  }));
  const groupOptions = groups.map((group) => {
    const members = profiles.filter((profile) => profile.groupId === group.id);
    return {
      id: group.id,
      kind: "group" as const,
      label: group.name,
      insertText: `@group:${quote(group.name)}`,
      available: members.length >= 2 && members.every((profile) => connectedCount(profile.id) === 1),
    };
  });
  return [...servers, ...groupOptions].sort((left, right) => {
    if (left.available !== right.available) return left.available ? -1 : 1;
    return left.label.localeCompare(right.label);
  });
}
