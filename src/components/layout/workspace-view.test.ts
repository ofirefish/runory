import { describe, expect, it } from "vitest";
import { restoreWorkspaceView } from "./workspace-view";

describe("workspace view restoration", () => {
  it.each(["assistant", "changes", "unknown", null])("opens the terminal for an unavailable saved view: %s", (saved) => {
    expect(restoreWorkspaceView(saved)).toBe("terminal");
  });

  it.each(["terminal", "files", "dashboard", "operations", "deployment"])("preserves the supported saved view: %s", (saved) => {
    expect(restoreWorkspaceView(saved)).toBe(saved);
  });
});
