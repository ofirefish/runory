import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../../i18n";
import { getAgentHistory, listAgentHistory } from "../../../lib/tauri/agent-v2";
import { getIncident, listIncidents } from "../../../lib/tauri/agentic";
import type { Incident } from "../../../types/agentic";
import type { AgentV2ResumableRun } from "../../../types/agent-v2";
import { HistoryDetails } from "./IncidentHistoryDetails";
import { HistoryList, IncidentHistoryPopover } from "./IncidentHistoryPopover";
import { historyDate, loadHistoryDetail, loadRecentHistory, type HistoryItem } from "./incident-history";

vi.mock("../../../lib/tauri/agent-v2", () => ({ listAgentHistory: vi.fn(), getAgentHistory: vi.fn() }));
vi.mock("../../../lib/tauri/agentic", () => ({ listIncidents: vi.fn(), getIncident: vi.fn() }));

const run: AgentV2ResumableRun = { run: { id: "run-1", state: "completed", createdAtEpochMs: 100, updatedAtEpochMs: 200, nextEventSeq: 3 }, goal: "Check disk", targetIds: ["server-1"] };
const incident: Incident = {
  id: "incident-1", agentRunId: "legacy-1", model: "local", pack: "disk", status: "interrupted", severity: "sev3", targets: ["session-1"], symptoms: [],
  evidence: [], evidenceReferences: [], comparisons: [], hypotheses: [],
  rootCause: { code: "incident-inconclusive", confidence: 0, evidenceIds: [] },
  proposedFix: { code: "continue-investigation", risk: "R1", actionCodes: [], changeSetRequired: true, executionPipeline: [], dangerousDeletionBlocked: true },
  changeSet: null, verification: { statusCode: "not-started", evidenceIds: [] }, resolution: null, handoff: null,
  timeline: [{ status: "reported", code: "incident-reported", occurredAtEpochMs: 10 }, { status: "interrupted", code: "process-recovery-interrupted", occurredAtEpochMs: 300 }],
  durationMs: 999999, recoveryState: "metadata-only",
  metrics: { runId: "legacy-1", incidentId: "incident-1", modelCalls: 0, inputTokens: 0, outputTokens: 0, toolCalls: 0, duplicateCalls: 0, mcpCalls: 0, contextSizeBytes: 0, compactionCount: 0, durationMs: 0, diagnosisLatencyMs: null, resolutionLatencyMs: null, verificationResult: null, rollbackResult: null, estimatedCostMicrousd: null, cacheHits: 0, parallelReadBatches: 0 },
};
const runItem: HistoryItem = { kind: "run", run, id: run.run.id, title: run.goal, status: run.run.state, updatedAt: 200, targetIds: run.targetIds };

beforeEach(async () => {
  vi.resetAllMocks();
  await i18n.changeLanguage("en-US");
  vi.mocked(listAgentHistory).mockResolvedValue([run]);
  vi.mocked(listIncidents).mockResolvedValue([incident]);
});

describe("recent incident history", () => {
  it("combines real run and incident records using lifecycle timestamps", async () => {
    const result = await loadRecentHistory(null);
    expect(result.items.map((item) => item.id)).toEqual(["incident-1", "run-1"]);
    expect(result.items[0].updatedAt).toBe(300);
    expect(result.partial).toBe(false);
  });

  it("filters incidents and sends the exact target filter to Rust", async () => {
    vi.mocked(listAgentHistory).mockResolvedValue([]);
    expect((await loadRecentHistory("server-2")).items).toEqual([]);
    expect(listAgentHistory).toHaveBeenCalledWith("server-2");
    expect((await loadRecentHistory("server-1", "session-1")).items.map((item) => item.id)).toEqual(["incident-1"]);
    expect((await loadRecentHistory("server-1", "session-2")).items).toEqual([]);
  });

  it("keeps available history after a partial failure and can retry", async () => {
    vi.mocked(listAgentHistory).mockRejectedValueOnce(new Error("unavailable"));
    expect(await loadRecentHistory(null)).toMatchObject({ partial: true, items: [{ id: "incident-1" }] });
    expect((await loadRecentHistory(null)).partial).toBe(false);
    vi.mocked(listAgentHistory).mockRejectedValue(new Error("unavailable"));
    vi.mocked(listIncidents).mockRejectedValue(new Error("unavailable"));
    await expect(loadRecentHistory(null)).rejects.toThrow("HISTORY_UNAVAILABLE");
  });

  it("opens run details using only the read-only history command", async () => {
    const detail = { run: run.run, events: [], truncated: false };
    vi.mocked(getAgentHistory).mockResolvedValue(detail);
    expect(await loadHistoryDetail(runItem)).toEqual({ kind: "run", detail });
    expect(getAgentHistory).toHaveBeenCalledWith("run-1");
    expect(getIncident).not.toHaveBeenCalled();
  });

  it("opens legacy records through the read-only incident get command", async () => {
    vi.mocked(getIncident).mockResolvedValue(incident);
    const item = (await loadRecentHistory(null)).items[0];
    expect(await loadHistoryDetail(item)).toEqual({ kind: "incident", detail: incident });
    expect(getIncident).toHaveBeenCalledWith("incident-1");
  });

  it("does not offer approval or pretend a historical proposal is executing", () => {
    const markup = renderToStaticMarkup(<HistoryDetails item={runItem} result={{ kind: "run", detail: { run: run.run, truncated: false, events: [
      { runId: "run-1", seq: 1, timestampEpochMs: 100, event: { type: "command_proposed", payload: { command_id: "cmd-1", command: "df -h", reason: "Inspect disk" } } },
      { runId: "run-1", seq: 2, timestampEpochMs: 200, event: { type: "command_approval_required", payload: { command_id: "cmd-1", approval_id: "old-approval" } } },
    ] } }} />);
    expect(markup).toContain("Recorded command");
    expect(markup).toContain("df -h");
    expect(markup).not.toContain("<button");
    expect(markup).not.toContain("agent-command-result-card running");
  });

  it("labels recovered metadata without fabricating symptoms or evidence", async () => {
    const item = (await loadRecentHistory(null)).items[0];
    const markup = renderToStaticMarkup(<HistoryDetails item={item} result={{ kind: "incident", detail: incident }} />);
    expect(markup).toContain(i18n.t("incident.metadataOnly"));
    expect(markup).toContain(i18n.t("incident.status.interrupted"));
    expect(markup).not.toContain("<button");
  });

  it("renders loading separately from empty history and localizes dates", async () => {
    expect(renderToStaticMarkup(<IncidentHistoryPopover onClose={() => {}} onSelect={() => {}} targetId={null} />)).toContain(i18n.t("common.loading"));
    expect(renderToStaticMarkup(<HistoryList items={[]} language="en-US" onSelect={() => {}} />)).toContain(i18n.t("contextPanel.historyEmpty"));
    const date = Date.UTC(2026, 8, 4, 10);
    expect(historyDate(date, "zh-CN")).not.toBe(historyDate(date, "en-US"));
    expect(historyDate(0, "en-US")).toBeNull();
    await i18n.changeLanguage("zh-CN");
    expect(renderToStaticMarkup(<HistoryList items={[runItem]} language="zh-CN" onSelect={() => {}} />)).toContain("已完成");
  });
});
