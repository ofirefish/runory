import type { AiTask } from "../../types/ai";

export const aiTaskNeedsInput = (task: AiTask) => task === "explain-command" || task === "generate-command";
export const aiTaskMayUseSessionContext = (task: AiTask) => task === "diagnose-output" || task === "propose-fix";
