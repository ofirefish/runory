import type { CloudConflictDecision, CloudConflictItem, CloudImportPreview } from "../../types/cloud";

export const conflictKey = (item: Pick<CloudConflictItem, "kind" | "id">) => `${item.kind}:${item.id}`;

export const buildConflictDecisions = (
  items: CloudConflictItem[],
  choices: Record<string, "keepLocal" | "useRemote">,
): CloudConflictDecision[] => items.map((item) => ({
  ...item,
  resolution: choices[conflictKey(item)] ?? "keepLocal",
}));

export const hasCloudSyncChanges = (preview: CloudImportPreview): boolean => [
  preview.groupAdditions,
  preview.groupUpdates,
  preview.profileAdditions,
  preview.profileUpdates,
  preview.groupDeletions,
  preview.profileDeletions,
  preview.localNewer,
  preview.conflicts,
].some((count) => count > 0);
