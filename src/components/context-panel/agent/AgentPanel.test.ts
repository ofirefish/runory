import { describe, expect, it } from "vitest";

/** Mirrors the Rust routing heuristic used by AR2-E (frontend no longer owns routing). */
function planningHintsFromGoalLike(text: string) {
  const url = text.match(/https?:\/\/[^\s<>'"]+/i)?.[0] ?? null;
  const service = text.match(/(?:service|systemd)\s*:\s*([a-zA-Z0-9_.@-]+)/i)?.[1] ?? null;
  return { url, service, includeNginxTest: /nginx|website|gateway/i.test(text) };
}

describe("AR2-E timeline presentation contract", () => {
  it("keeps routing hints aligned with the Rust heuristic", () => {
    expect(planningHintsFromGoalLike("Check service: nginx at https://example.com/health")).toEqual({
      service: "nginx",
      url: "https://example.com/health",
      includeNginxTest: true,
    });
  });

  it("does not invent optional targets", () => {
    expect(planningHintsFromGoalLike("Why is this server slow?")).toEqual({
      service: null,
      url: null,
      includeNginxTest: false,
    });
  });
});
