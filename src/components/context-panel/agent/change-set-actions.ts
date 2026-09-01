import type { ChangeSet } from "../../../types/agentic";

export type InlineChangeSetCapabilities = {
  canApprove: boolean;
  canApproveSteps: boolean;
  canExecute: boolean;
  canReject: boolean;
  canRollback: boolean;
};

/** Mirrors the server-side state machine so stale or recovered snapshots never
 * advertise an unsafe action. The Rust command remains the authority. */
export function getInlineChangeSetCapabilities(changeSet: ChangeSet): InlineChangeSetCapabilities {
  const live = changeSet.recoveryState === "live";
  const decision = changeSet.policyEvaluation?.decision;
  const draft = changeSet.approvalState === "draft" && changeSet.executionState === "not-started";
  const supportsRollback = changeSet.steps.some((step) => step.rollbackCapability !== "not-supported");

  return {
    canApprove: live && draft && decision !== "DENY" && decision !== "REQUIRE_STEP_APPROVAL",
    canApproveSteps: live && draft && decision === "REQUIRE_STEP_APPROVAL",
    canExecute: live && changeSet.approvalState === "approved" && changeSet.executionState === "not-started",
    canReject: live && draft,
    canRollback: live && supportsRollback && (changeSet.executionState === "committed" || changeSet.executionState === "failed"),
  };
}
