import type { AiToolInput, AiToolName, AiToolSummary } from "../../types/ai-agent";

export const toolNeedsPath = (tool: AiToolName) => tool === "file-read" || tool === "file-write";
export const toolNeedsContent = (tool: AiToolName) => tool === "file-write";
export const toolNeedsContainer = (tool: AiToolName) => tool === "docker-restart";
export const describeToolTarget = (tool: AiToolSummary | AiToolInput) => {
  if (tool.tool === "file-read" || tool.tool === "file-write") return tool.path;
  if (tool.tool === "docker-restart") return tool.container;
  if (tool.tool === "terminal-exec") return tool.preset;
  return null;
};
