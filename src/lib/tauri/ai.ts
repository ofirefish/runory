import { invoke } from "@tauri-apps/api/core";
import type { AiAssistantResponse } from "../../types/ai";

export const explainCommand = (sessionId: string, command: string) => invoke<AiAssistantResponse>("ai_explain_command", { request: { sessionId, command } });
export const generateCommand = (sessionId: string, intent: string) => invoke<AiAssistantResponse>("ai_generate_command", { request: { sessionId, intent } });
export const diagnoseOutput = (sessionId: string, output?: string) => invoke<AiAssistantResponse>("ai_diagnose_output", { request: { sessionId, output: output?.trim() || null } });
export const proposeFix = (sessionId: string, output?: string) => invoke<AiAssistantResponse>("ai_propose_fix", { request: { sessionId, output: output?.trim() || null } });
