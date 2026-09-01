import type { CloudConflictDecision, CloudConflictItem } from "../../types/cloud";

export const conflictKey = (item: Pick<CloudConflictItem, "kind" | "id">) => `${item.kind}:${item.id}`;

export const buildConflictDecisions = (
  items: CloudConflictItem[],
  choices: Record<string, "keepLocal" | "useRemote">,
): CloudConflictDecision[] => items.map((item) => ({
  ...item,
  resolution: choices[conflictKey(item)] ?? "keepLocal",
}));
