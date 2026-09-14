import { describe, expect, it } from "vitest";
import { fleetChangeSetCanRollback } from "./fleet-changeset-view";

const review = (executionState: string, targetState: string, stepState: string, rollbackCapability: string) => ({
  execution: {
    executionState,
    targets: [{ targetId: "session-1", state: targetState }],
  },
  targets: [{
    sessionId: "session-1",
    changeSet: { steps: [{ state: stepState, rollbackCapability }] },
  }],
});

describe("fleetChangeSetCanRollback", () => {
  it("requires a completed target and a succeeded reversible step", () => {
    expect(fleetChangeSetCanRollback(review("failed", "succeeded", "succeeded", "file-restore"))).toBe(true);
    expect(fleetChangeSetCanRollback(review("failed", "failed", "succeeded", "file-restore"))).toBe(false);
    expect(fleetChangeSetCanRollback(review("failed", "succeeded", "failed", "file-restore"))).toBe(false);
    expect(fleetChangeSetCanRollback(review("failed", "succeeded", "succeeded", "not-supported"))).toBe(false);
  });

  it("does not expose rollback while execution is active", () => {
    expect(fleetChangeSetCanRollback(review("executing", "succeeded", "succeeded", "file-restore"))).toBe(false);
  });
});
