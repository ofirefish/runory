import { describe, expect, it } from "vitest";
import { inferDiagnosticInputs } from "./agent-routing";

describe("AgentPanel diagnostic routing", () => {
  it("extracts only bounded typed hints from the natural-language request", () => {
    expect(inferDiagnosticInputs("Check service: nginx at https://example.com/health")).toEqual({
      service: "nginx",
      url: "https://example.com/health",
      includeNginxTest: true,
    });
  });

  it("does not invent optional targets", () => {
    expect(inferDiagnosticInputs("Why is this server slow?")).toEqual({
      service: null,
      url: null,
      includeNginxTest: false,
    });
  });
});
