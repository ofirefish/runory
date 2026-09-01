import { describe, expect, it } from "vitest";
import { aiTaskMayUseSessionContext, aiTaskNeedsInput } from "./ai-task";

describe("AI task policy", () => {
  it("requires explicit text for command analysis and generation", () => {
    expect(aiTaskNeedsInput("explain-command")).toBe(true);
    expect(aiTaskNeedsInput("generate-command")).toBe(true);
  });

  it("only permits diagnostic tasks to use bounded terminal context", () => {
    expect(aiTaskMayUseSessionContext("diagnose-output")).toBe(true);
    expect(aiTaskMayUseSessionContext("propose-fix")).toBe(true);
    expect(aiTaskMayUseSessionContext("generate-command")).toBe(false);
  });
});
