import { describe, expect, it } from "vitest";
import type { ChangeSet, PolicyDecision } from "../../../types/agentic";
import { getInlineChangeSetCapabilities } from "./change-set-actions";

function changeSet(overrides: Partial<ChangeSet> = {}, decision: PolicyDecision = "REQUIRE_APPROVAL"): ChangeSet {
  return {
    id: "change-1", agentRunId: "run-1", sessionId: "session-1", title: "Restart service", version: 1, risk: "R3",
    approvalState: "draft", approvedVersion: null, approvedStepIds: [], executionState: "not-started", recoveryState: "live",
    preconditions: [], policySnapshot: null,
    policyEvaluation: { decision, matchedRules: [], reason: "DEFAULT_REQUIRE_APPROVAL", scope: { kind: "global" }, policyVersion: 1, policyHash: "hash" },
    steps: [{ id: "step-1", order: 1, toolName: "service.restart", risk: "R3", preview: "Restart nginx", verificationPlanCode: "SERVICE_ACTIVE", rollbackCapability: "not-supported", state: "pending", errorCode: null }],
    ...overrides,
  };
}

describe("inline ChangeSet action availability", () => {
  it("offers approval and rejection only for a live policy-checked draft", () => {
    expect(getInlineChangeSetCapabilities(changeSet())).toMatchObject({ canApprove: true, canReject: true, canExecute: false });
    expect(getInlineChangeSetCapabilities(changeSet({}, "DENY"))).toMatchObject({ canApprove: false, canReject: true });
  });

  it("requires step approval when policy says so", () => {
    expect(getInlineChangeSetCapabilities(changeSet({}, "REQUIRE_STEP_APPROVAL"))).toMatchObject({ canApprove: false, canApproveSteps: true });
  });

  it("offers execution only after exact-version approval", () => {
    expect(getInlineChangeSetCapabilities(changeSet({ approvalState: "approved", approvedVersion: 1 }))).toMatchObject({ canExecute: true, canApprove: false, canReject: false });
  });

  it("never claims rollback for unsupported steps or metadata-only recovery", () => {
    expect(getInlineChangeSetCapabilities(changeSet({ executionState: "committed" })).canRollback).toBe(false);
    expect(getInlineChangeSetCapabilities(changeSet({ executionState: "committed", steps: [{ ...changeSet().steps[0], toolName: "file.patch", rollbackCapability: "reverse-patch" }] })).canRollback).toBe(true);
    expect(getInlineChangeSetCapabilities(changeSet({ executionState: "committed", recoveryState: "metadata-only", steps: [{ ...changeSet().steps[0], rollbackCapability: "reverse-patch" }] })).canRollback).toBe(false);
  });
});
