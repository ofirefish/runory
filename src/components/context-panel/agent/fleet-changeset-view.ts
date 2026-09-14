type RollbackReview = {
  execution: {
    executionState: string;
    targets: Array<{ targetId: string; state: string }>;
  };
  targets: Array<{
    sessionId: string;
    changeSet: { steps: Array<{ state: string; rollbackCapability: string }> };
  }>;
};

export function fleetChangeSetCanRollback(review: RollbackReview) {
  if (!["paused-for-review", "failed", "succeeded"].includes(review.execution.executionState)) {
    return false;
  }
  const succeededTargets = new Set(
    review.execution.targets
      .filter((target) => target.state === "succeeded")
      .map((target) => target.targetId),
  );
  return review.targets.some((target) =>
    succeededTargets.has(target.sessionId)
    && target.changeSet.steps.some((step) =>
      step.state === "succeeded" && step.rollbackCapability !== "not-supported"),
  );
}
