import { create } from "zustand";
import type { WorkflowPlanDto, WorkflowRunDto, WorkflowStepStatusDto } from "../types/plugin";
import {
  pluginWorkflowCancel,
  pluginWorkflowExecute,
  pluginWorkflowPlan,
} from "../../../lib/tauri/plugins";
import { appErrorCode } from "../../../lib/app-error";

type WorkflowRunStore = {
  plan: WorkflowPlanDto | null;
  run: WorkflowRunDto | null;
  steps: WorkflowStepStatusDto[];
  busy: boolean;
  /** Stable error code key: planFailed / executeFailed / PLUGIN_* / UNKNOWN */
  error: string | null;
  planWorkflow: (args: {
    pluginId: string;
    workflowId: string;
    hostId?: string;
    sessionId?: string;
    inputs?: Record<string, unknown>;
  }) => Promise<WorkflowPlanDto>;
  executePlan: (planId: string) => Promise<WorkflowRunDto>;
  cancelRun: (runId: string) => Promise<void>;
  reset: () => void;
};

export const useWorkflowRunStore = create<WorkflowRunStore>((set) => ({
  plan: null,
  run: null,
  steps: [],
  busy: false,
  error: null,
  planWorkflow: async (args) => {
    set({ busy: true, error: null });
    try {
      const plan = await pluginWorkflowPlan({
        pluginId: args.pluginId,
        workflowId: args.workflowId,
        hostId: args.hostId,
        sessionId: args.sessionId,
        inputs: args.inputs ?? {},
      });
      set({ plan, busy: false });
      return plan;
    } catch (error) {
      const code = appErrorCode(error);
      set({
        busy: false,
        error: code === "UNKNOWN" ? "planFailed" : code,
      });
      throw error;
    }
  },
  executePlan: async (planId) => {
    set({ busy: true, error: null });
    try {
      const run = await pluginWorkflowExecute(planId);
      set({ run, steps: run.steps ?? [], busy: false });
      return run;
    } catch (error) {
      const code = appErrorCode(error);
      set({
        busy: false,
        error: code === "UNKNOWN" ? "executeFailed" : code,
      });
      throw error;
    }
  },
  cancelRun: async (runId) => {
    await pluginWorkflowCancel(runId);
  },
  reset: () => set({ plan: null, run: null, steps: [], busy: false, error: null }),
}));
