import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "../../../i18n";
import type { AgentEventEnvelope } from "../../../types/agent-v2";
import { AgentRunError } from "./AgentRunError";
import { AgentTimeline } from "./AgentTimeline";
import { emptyTimeline, mergeAgentEvent } from "./agent-timeline-utils";

const failure: AgentEventEnvelope = {
  runId: "run-1", seq: 1, timestampEpochMs: 1000,
  event: { type: "run_failed", payload: { error_code: "MODEL_UNAVAILABLE" } },
};

describe("Agent run error presentation", () => {
  beforeEach(async () => { await i18n.changeLanguage("zh-CN"); });

  it("renders one alert when a failure is in both timeline and error state", () => {
    const model = mergeAgentEvent(emptyTimeline("run-1"), failure);
    const markup = renderToStaticMarkup(<>
      <AgentTimeline events={model.events} expanded={{}} onToggle={() => {}} onApprove={() => {}} onReject={() => {}} />
      <AgentRunError events={model.events} lastErrorCode={model.lastErrorCode} running={false} />
    </>);
    expect(markup.match(/role="alert"/g)).toHaveLength(1);
    expect(markup).toContain(i18n.t("contextPanel.error.MODEL_UNAVAILABLE"));
  });

  it("retains replayed failures even when later metadata clears the transient error", () => {
    const markup = renderToStaticMarkup(<AgentRunError events={[failure]} lastErrorCode={null} running={false} />);
    expect(markup).toContain('role="alert"');
  });

  it("shows IPC errors without requiring a run failure event", () => {
    const markup = renderToStaticMarkup(<AgentRunError events={[]} lastErrorCode="MODEL_AUTH_FAILED" running={false} />);
    expect(markup).toContain(i18n.t("contextPanel.error.MODEL_AUTH_FAILED"));
  });

  it("hides errors while retrying and after clearing the conversation", () => {
    expect(renderToStaticMarkup(<AgentRunError events={[failure]} lastErrorCode="MODEL_UNAVAILABLE" running />)).toBe("");
    expect(renderToStaticMarkup(<AgentRunError events={[]} lastErrorCode={null} running={false} />)).toBe("");
  });
});
