import { getAgentHistory, listAgentHistory } from "../../../lib/tauri/agent-v2";
import { getIncident, listIncidents } from "../../../lib/tauri/agentic";
import type { AgentHistoryDetail, AgentV2ResumableRun } from "../../../types/agent-v2";
import type { Incident } from "../../../types/agentic";

export type HistoryItem = {
  id: string; title: string; status: string; targetIds: string[]; updatedAt: number;
} & ({ kind: "run"; run: AgentV2ResumableRun } | { kind: "incident"; incident: Incident });

export type HistoryDetail = { kind: "run"; detail: AgentHistoryDetail } | { kind: "incident"; detail: Incident };

export async function loadRecentHistory(targetId: string | null, sessionId: string | null = null): Promise<{ items: HistoryItem[]; partial: boolean }> {
  const [runs, incidents] = await Promise.allSettled([listAgentHistory(targetId), listIncidents()]);
  if (runs.status === "rejected" && incidents.status === "rejected") throw new Error("HISTORY_UNAVAILABLE");
  const items: HistoryItem[] = [];
  if (runs.status === "fulfilled") {
    for (const run of runs.value) {
      items.push({ kind: "run", run, id: run.run.id, title: run.goal, status: run.run.state,
        targetIds: run.targetIds, updatedAt: run.run.updatedAtEpochMs });
    }
  }
  if (incidents.status === "fulfilled") {
    for (const incident of incidents.value) {
      // Legacy incidents bind session IDs; V2 runs bind persistent server IDs.
      if (targetId && (!sessionId || !incident.targets.includes(sessionId))) continue;
      items.push({ kind: "incident", incident, id: incident.id, title: incident.symptoms[0] ?? "",
        status: incident.status, targetIds: incident.targets,
        updatedAt: Math.max(0, ...incident.timeline.map((event) => event.occurredAtEpochMs)) });
    }
  }
  items.sort((a, b) => b.updatedAt - a.updatedAt || a.id.localeCompare(b.id));
  return { items: items.slice(0, 50), partial: runs.status === "rejected" || incidents.status === "rejected" };
}

export async function loadHistoryDetail(item: HistoryItem): Promise<HistoryDetail> {
  return item.kind === "run"
    ? { kind: "run", detail: await getAgentHistory(item.id) }
    : { kind: "incident", detail: await getIncident(item.id) };
}

export function historyStatusKey(item: Pick<HistoryItem, "kind" | "status">): string {
  return item.kind === "incident" ? `incident.status.${item.status}` : `contextPanel.historyState.${item.status}`;
}

export function historyDate(epochMs: number, language: string): string | null {
  if (!Number.isFinite(epochMs) || epochMs <= 0) return null;
  return new Intl.DateTimeFormat(language, { dateStyle: "medium", timeStyle: "short" }).format(epochMs);
}
