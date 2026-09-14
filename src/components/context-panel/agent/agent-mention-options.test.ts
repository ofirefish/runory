import { describe, expect, it } from "vitest";
import type { HostGroup, ServerProfile } from "../../../types/domain";
import type { SessionTab } from "../../../stores/session-store";
import { buildAgentMentionOptions } from "./agent-mention-options";

function profile(id: string, name: string, groupId: string | null): ServerProfile {
  return {
    id,
    name,
    host: `${id}.example.test`,
    port: 22,
    username: "ops",
    groupId,
    authMethod: "privateKey",
    connectionRoute: { type: "direct" },
    sortOrder: 0,
    createdAt: "",
    updatedAt: "",
  };
}

function tab(profileId: string, suffix = "1", state: SessionTab["state"] = "connected"): SessionTab {
  return {
    id: `tab-${profileId}-${suffix}`,
    profileId,
    sessionId: state === "connected" ? `session-${profileId}-${suffix}` : null,
    connectionAttemptId: `attempt-${profileId}-${suffix}`,
    state,
    view: "agent",
  };
}

describe("agent mention options", () => {
  it("marks only exact single-session servers and fully connected groups available", () => {
    const profiles = [
      profile("primary", "db-primary", "db"),
      profile("replica", "db replica", "db"),
      profile("ambiguous", "worker", null),
    ];
    const groups: HostGroup[] = [
      { id: "db", name: "Database Nodes", sortOrder: 0, collapsed: false, createdAt: "", updatedAt: "" },
    ];
    const options = buildAgentMentionOptions(profiles, groups, [
      tab("primary"),
      tab("replica"),
      tab("ambiguous", "1"),
      tab("ambiguous", "2"),
    ]);

    expect(options.find((option) => option.id === "primary")?.available).toBe(true);
    expect(options.find((option) => option.id === "ambiguous")?.available).toBe(false);
    expect(options.find((option) => option.id === "db")).toMatchObject({
      available: true,
      insertText: '@group:"Database Nodes"',
    });
    expect(options.find((option) => option.id === "replica")?.insertText).toBe('@"db replica"');
  });

  it("keeps a group unavailable if any member is disconnected", () => {
    const profiles = [profile("one", "one", "group"), profile("two", "two", "group")];
    const groups: HostGroup[] = [
      { id: "group", name: "group", sortOrder: 0, collapsed: false, createdAt: "", updatedAt: "" },
    ];
    const options = buildAgentMentionOptions(profiles, groups, [tab("one"), tab("two", "1", "disconnected")]);
    expect(options.find((option) => option.kind === "group")?.available).toBe(false);
  });
});
