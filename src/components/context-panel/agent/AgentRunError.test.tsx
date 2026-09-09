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

  it("hides replayed run failures while retrying and after clearing the conversation", () => {
    expect(renderToStaticMarkup(<AgentRunError events={[failure]} lastErrorCode={null} running />)).toBe("");
    expect(renderToStaticMarkup(<AgentRunError events={[]} lastErrorCode={null} running={false} />)).toBe("");
  });

  it("renders the cached account avatar beside a user message", () => {
    const userMessage: AgentEventEnvelope = {
      runId: "run-1", seq: 1, timestampEpochMs: 1000,
      event: { type: "user_message_added", payload: { content: "Install PM2" } },
    };
    const markup = renderToStaticMarkup(
      <AgentTimeline events={[userMessage]} userAvatarUrl="data:image/webp;base64,cached" expanded={{}} onToggle={() => {}} onApprove={() => {}} onReject={() => {}} />,
    );
    expect(markup).toContain('class="agent-user-avatar"');
    expect(markup).toContain('src="data:image/webp;base64,cached"');
  });

  it("shows an approval IPC failure while the run remains awaiting approval", () => {
    const markup = renderToStaticMarkup(
      <AgentRunError events={[]} lastErrorCode="INVALID_OPERATION" running />,
    );
    expect(markup).toContain('role="alert"');
    expect(markup).not.toContain("agent-error-retry");
  });

  it("offers retry for transient model failures", () => {
    const markup = renderToStaticMarkup(
      <AgentRunError events={[failure]} lastErrorCode="MODEL_UNAVAILABLE" running={false} onRetry={() => undefined} />,
    );
    expect(markup).toContain(i18n.t("contextPanel.retry"));
    expect(markup).toContain("agent-error-retry");
  });

  it("renders precise managed response failures", () => {
    const emptyFailure: AgentEventEnvelope = {
      ...failure,
      event: { type: "run_failed", payload: { error_code: "MODEL_RESPONSE_EMPTY" } },
    };
    const markup = renderToStaticMarkup(
      <AgentRunError events={[emptyFailure]} lastErrorCode="MODEL_RESPONSE_EMPTY" running={false} onRetry={() => undefined} />,
    );
    expect(markup).toContain(i18n.t("contextPanel.error.MODEL_RESPONSE_EMPTY"));
    expect(markup).toContain(i18n.t("contextPanel.retry"));
  });

  it("hides retry for auth failures", () => {
    const markup = renderToStaticMarkup(
      <AgentRunError events={[]} lastErrorCode="MODEL_AUTH_FAILED" running={false} onRetry={() => undefined} />,
    );
    expect(markup).not.toContain("agent-error-retry");
  });
});
