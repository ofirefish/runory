import { describe, expect, it } from "vitest";
import type { AgentEventEnvelope } from "../../../types/agent-v2";
import { deriveRunState, emptyTimeline, mergeAgentEvent, pendingApprovalFromEvents, reasoningRound, runElapsedSeconds, timelinePhase } from "./agent-timeline-utils";

function envelope(seq: number, type: AgentEventEnvelope["event"]["type"], payload: Record<string, unknown> = {}): AgentEventEnvelope {
  return {
    runId: "run-1",
    seq,
    timestampEpochMs: seq * 1000,
    event: { type, payload },
  };
}

describe("agent timeline utils", () => {
  it("appends events in sequence order without duplicates", () => {
    const base = emptyTimeline("run-1");
    const first = mergeAgentEvent(base, envelope(1, "run_started"));
    const second = mergeAgentEvent(first, envelope(2, "progress_updated", { summary: "Checking disk" }));
    const duplicate = mergeAgentEvent(second, envelope(2, "progress_updated", { summary: "Checking disk" }));
    expect(second.events).toHaveLength(2);
    expect(duplicate.events).toHaveLength(2);
    expect(second.currentApproach).toBe("Checking disk");
  });

  it("tracks pending approval until resolved", () => {
    const events = [
      envelope(1, "tool_approval_required", { tool_call_id: "tc-1", tool_name: "service.logs" }),
      envelope(2, "approval_granted", { approval_id: "ap-1" }),
    ];
    expect(pendingApprovalFromEvents(events.slice(0, 1))?.toolName).toBe("service.logs");
    expect(pendingApprovalFromEvents(events)).toBeUndefined();
  });

  it("derives terminal and interrupt states from the tail", () => {
    const events = [
      envelope(1, "run_started"),
      envelope(2, "tool_approval_required", { tool_call_id: "tc-1" }),
    ];
    expect(deriveRunState(events)).toBe("awaiting_approval");
    expect(deriveRunState([...events, envelope(3, "run_completed", { summary: "Done" })])).toBe("completed");
  });

  it("marks change set approvals after change_set_proposed", () => {
    const events = [
      envelope(1, "change_set_proposed", { change_set_id: "cs-1" }),
      envelope(2, "tool_approval_required", { tool_call_id: "tc-1", tool_name: "Reload nginx" }),
    ];
    expect(pendingApprovalFromEvents(events)).toMatchObject({
      kind: "change_set",
      changeSetId: "cs-1",
      toolName: "Reload nginx",
    });
  });

  it("marks paused runs as not running", () => {
    const paused = mergeAgentEvent(emptyTimeline("run-1"), envelope(1, "run_paused"));
    expect(paused.running).toBe(false);
    expect(deriveRunState(paused.events)).toBe("paused");
  });

  it("reconstructs an exact command approval card and clears it after approval", () => {
    const events = [
      envelope(1, "command_proposed", {
        command_id: "cmd-1",
        command: "df -h",
        reason: "Inspect filesystem usage",
        risk: "low",
        mutability: "read",
      }),
      envelope(2, "command_approval_required", {
        approval_id: "approval-1",
        command_id: "cmd-1",
      }),
    ];
    expect(pendingApprovalFromEvents(events)).toMatchObject({
      approvalId: "approval-1",
      kind: "command",
      command: "df -h",
      reason: "Inspect filesystem usage",
      risk: "low",
      mutability: "read",
    });
    expect(deriveRunState(events)).toBe("awaiting_approval");

    const approved = [...events, envelope(3, "approval_granted", { approval_id: "approval-1" }), envelope(4, "command_started", { command_id: "cmd-1" })];
    expect(pendingApprovalFromEvents(approved)).toBeUndefined();
    expect(deriveRunState(approved)).toBe("acting");
  });

  it("derives the exact command timeline phases and second reasoning round", () => {
    const events = [
      envelope(1, "run_created"),
      envelope(2, "reasoning_started"),
    ];
    expect(timelinePhase(events)).toBe("thinking");
    expect(reasoningRound(events)).toBe(1);

    const awaiting = [...events, envelope(3, "command_approval_required", { command_id: "cmd-1" })];
    expect(timelinePhase(awaiting)).toBe("awaiting_approval");

    const analyzing = [
      ...awaiting,
      envelope(4, "approval_granted"),
      envelope(5, "command_started", { command_id: "cmd-1" }),
      envelope(6, "command_completed", { command_id: "cmd-1", output_preview: "37MiB file" }),
      envelope(7, "reasoning_started"),
    ];
    expect(timelinePhase(analyzing)).toBe("analyzing");
    expect(reasoningRound(analyzing)).toBe(2);
    expect(runElapsedSeconds(analyzing, 9_500)).toBe(8);
  });
});
