import { describe, expect, it } from "vitest";
import type { CloudConflictItem } from "../../types/cloud";
import { buildConflictDecisions, conflictKey } from "./cloud-sync";

const item: CloudConflictItem = {
  kind: "profile",
  id: "6d92efb9-cd9f-4698-b960-86b8c5ac8ee6",
  label: "API",
  localUpdatedAt: "10",
  remoteUpdatedAt: "9",
  remoteDeleted: false,
};

describe("cloud conflict decisions", () => {
  it("defaults every unresolved conflict to keeping local data", () => {
    expect(buildConflictDecisions([item], {})[0].resolution).toBe("keepLocal");
  });

  it("binds an explicit choice to object kind and id", () => {
    expect(buildConflictDecisions([item], { [conflictKey(item)]: "useRemote" })[0]).toMatchObject({
      id: item.id,
      localUpdatedAt: "10",
      resolution: "useRemote",
    });
  });
});
