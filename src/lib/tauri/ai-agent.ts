import { invoke } from "@tauri-apps/api/core";
import type { AiAgentPlan, AiAuditRecord, AiToolExecution, AiToolInput } from "../../types/ai-agent";

export const createAgentPlan = (sessionIds: string[], goal: string, tools: AiToolInput[]) => invoke<AiAgentPlan>("ai_agent_plan_create", { request: { sessionIds, goal, tools } });
export const getAgentPlan = (planId: string) => invoke<AiAgentPlan>("ai_agent_plan_get", { request: { planId } });
export const discardAgentPlan = (planId: string) => invoke<void>("ai_agent_plan_discard", { request: { planId } });
export const approveAgentStep = (planId: string, stepId: string) => invoke<AiAgentPlan>("ai_agent_step_approve", { request: { planId, stepId } });
export const executeAgentStep = (planId: string, stepId: string) => invoke<AiToolExecution>("ai_agent_step_execute", { request: { planId, stepId } });
export const listAgentAudit = (profileId?: string) => invoke<AiAuditRecord[]>("ai_agent_audit_list", { request: { profileId: profileId ?? null } });
