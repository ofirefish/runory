import { describe, expect, it } from "vitest";

describe("marketplace api path encoding", () => {
  it("builds download urls matching Core client expectations", () => {
    const pluginId = "runory.hello";
    const version = "1.0.0";
    const downloadUrl = `/api/v1/plugins/${encodeURIComponent(pluginId)}/versions/${encodeURIComponent(version)}/download`;
    expect(downloadUrl).toBe("/api/v1/plugins/runory.hello/versions/1.0.0/download");
  });
});
