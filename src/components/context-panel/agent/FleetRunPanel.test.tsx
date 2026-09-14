import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "../../../i18n";
import type { FleetApprovalV2, FleetRunV2 } from "../../../types/agent-v2";
import { FleetRunPanel } from "./FleetRunPanel";
import { fleetStageProgress } from "./fleet-run-view";

const run: FleetRunV2 = {
  id: "fleet-1",
  version: 2,
  state: "awaiting_approval",
  production: true,
  failurePolicy: "pause_for_review",
  targets: [
    { profileId: "primary", sessionId: "session-primary", role: "source", ordinal: 0 },
    { profileId: "replica", sessionId: "session-replica", role: "replica", ordinal: 1 },
  ],
  stages: [
    {
      id: "inspect",
      summary: "Inspect topology",
      targetIds: ["primary", "replica"],
      dependsOn: [],
      executionStrategy: "sequential",
      concurrencyLimit: 1,
      state: "awaiting_approval",
    },
    {
      id: "configure",
      summary: "Configure replicas",
      targetIds: ["replica"],
      dependsOn: ["inspect"],
      executionStrategy: "canary",
      concurrencyLimit: 1,
      state: "pending",
    },
  ],
  children: [
    { stageId: "inspect", targetId: "primary", agentRunId: null, attempt: 0, state: "succeeded", errorCode: null },
    { stageId: "inspect", targetId: "replica", agentRunId: "child-1", attempt: 1, state: "failed", errorCode: "CONNECTION_LOST" },
  ],
  graphDigest: "digest",
  recoveryState: "live",
  createdAtEpochMs: 1,
  updatedAtEpochMs: 2,
};

const approval: FleetApprovalV2 = {
  id: "approval-1",
  fleetRunId: run.id,
  fleetVersion: run.version,
  graphDigest: run.graphDigest,
  targets: run.targets,
  policyVersion: 1,
  policyHash: "policy",
  state: "pending",
  invalidationCode: null,
  createdAtEpochMs: 2,
  decidedAtEpochMs: null,
};

describe("FleetRunPanel", () => {
  beforeEach(async () => { await i18n.changeLanguage("en-US"); });

  it("renders stages, exact target roles and target-local status", () => {
    const markup = renderToStaticMarkup(<FleetRunPanel
      run={run}
      approval={approval}
      profileNames={{ primary: "db-primary", replica: "db-replica" }}
      onApprove={() => undefined}
      onReject={() => undefined}
    />);
    expect(markup).toContain("Inspect topology");
    expect(markup).toContain("Configure replicas");
    expect(markup).toContain("db-primary");
    expect(markup).toContain("source");
    expect(markup).toContain("CONNECTION_LOST");
    expect(markup).toContain(i18n.t("contextPanel.fleet.approve"));
    expect(markup).not.toContain('value="parallel"');
  });

  it("keeps recovered metadata read-only and exposes no action", () => {
    const recovered = { ...run, state: "interrupted" as const, recoveryState: "metadata_only" as const };
    const markup = renderToStaticMarkup(<FleetRunPanel run={recovered} profileNames={{}} />);
    expect(markup).not.toContain("<button");
    expect(markup).toContain(i18n.t("contextPanel.fleet.state.interrupted"));
  });

  it("maps stage states into stable presentation groups", () => {
    expect(fleetStageProgress("pending")).toBe("pending");
    expect(fleetStageProgress("verifying")).toBe("active");
    expect(fleetStageProgress("succeeded")).toBe("success");
    expect(fleetStageProgress("rollback_failed")).toBe("failure");
  });
});
