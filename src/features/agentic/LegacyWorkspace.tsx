import { AiAgentView } from "../ai-agent/AiAgentView";
import type { AgentSessionOption } from "./AgenticWorkspaceView";

export default function LegacyWorkspace({ sessions, activeSessionId }: { sessions: AgentSessionOption[]; activeSessionId: string | null }) {
  return <div className="h-full"><AiAgentView sessions={sessions} activeSessionId={activeSessionId} /></div>;
}
