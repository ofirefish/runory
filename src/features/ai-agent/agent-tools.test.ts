import { describe, expect, it } from "vitest";
import { describeToolTarget, toolNeedsContent, toolNeedsPath } from "./agent-tools";

describe("typed agent tools", () => {
  it("uses the redacted file summary in review labels", () => {
    expect(describeToolTarget({ tool: "file-write", path: "/tmp/app.env", bytes: 8 })).toBe("/tmp/app.env");
  });

  it("only asks file write for content", () => {
    expect(toolNeedsPath("file-read")).toBe(true);
    expect(toolNeedsContent("file-read")).toBe(false);
    expect(toolNeedsContent("file-write")).toBe(true);
  });
});
