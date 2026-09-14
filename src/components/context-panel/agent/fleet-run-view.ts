import type { FleetStageStateV2 } from "../../../types/agent-v2";

export function fleetStageProgress(state: FleetStageStateV2): "pending" | "active" | "success" | "failure" {
  if (["running", "awaiting_approval", "verifying", "rollback_pending"].includes(state)) return "active";
  if (["succeeded", "rolled_back"].includes(state)) return "success";
  if (["failed", "blocked", "cancelled", "rollback_failed"].includes(state)) return "failure";
  return "pending";
}
