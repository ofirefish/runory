import { describe, expect, it } from "vitest";
import type { HostGroup, ServerProfile } from "../../types/domain";
import type { SessionTab } from "../../stores/session-store";
import { parseFleetMentions, resolveFleetMentions } from "./fleet-mentions";

const groups: HostGroup[] = [
  { id: "g-db", name: "Database Nodes", sortOrder: 0, collapsed: false, createdAt: "", updatedAt: "" },
];

function profile(id: string, name: string, groupId: string | null = null): ServerProfile {
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

describe("fleet mention parsing", () => {
  it("parses server, quoted group and role mentions without treating email-like text as a mention", () => {
    const result = parseFleetMentions(
      'notify ops@example.com then use @db-primary#source and @group:"Database Nodes"#replica',
    );

    expect(result.errors).toEqual([]);
    expect(result.mentions).toMatchObject([
      { kind: "server", name: "db-primary", role: "source" },
      { kind: "group", name: "Database Nodes", role: "replica" },
    ]);
  });

  it("reports malformed quoted mentions and roles", () => {
    expect(parseFleetMentions('@"Database Primary').errors[0]?.code).toBe("unterminated-quote");
    expect(parseFleetMentions("@db-primary#").errors[0]?.code).toBe("invalid-role");
  });
});

describe("fleet mention resolution", () => {
  const profiles = [
    profile("primary", "db-primary"),
    profile("replica-1", "db-replica-1", "g-db"),
    profile("replica-2", "db-replica-2", "g-db"),
  ];

  it("expands groups into exact connected profile/session bindings", () => {
    const parsed = parseFleetMentions('@db-primary#source @group:"Database Nodes"#replica');
    const result = resolveFleetMentions(
      parsed.mentions,
      profiles,
      groups,
      profiles.map((item) => tab(item.id)),
    );

    expect(result.errors).toEqual([]);
    expect(result.targets).toEqual([
      { profileId: "primary", sessionId: "session-primary-1", displayName: "db-primary", role: "source", ordinal: 0 },
      { profileId: "replica-1", sessionId: "session-replica-1-1", displayName: "db-replica-1", role: "replica", ordinal: 1 },
      { profileId: "replica-2", sessionId: "session-replica-2-1", displayName: "db-replica-2", role: "replica", ordinal: 2 },
    ]);
  });

  it("rejects disconnected and multiply-connected targets", () => {
    const parsed = parseFleetMentions("@db-primary @db-replica-1");
    const result = resolveFleetMentions(parsed.mentions, profiles, groups, [
      tab("primary", "1"),
      tab("primary", "2"),
      tab("replica-1", "1", "disconnected"),
    ]);

    expect(result.errors.map((error) => error.code)).toEqual([
      "target-session-ambiguous",
      "target-disconnected",
    ]);
  });

  it("rejects ambiguous catalog names and conflicting role assignments", () => {
    const duplicate = profile("primary-copy", "DB-PRIMARY");
    const ambiguous = resolveFleetMentions(
      parseFleetMentions("@db-primary @db-replica-1").mentions,
      [...profiles, duplicate],
      groups,
      profiles.map((item) => tab(item.id)),
    );
    expect(ambiguous.errors[0]?.code).toBe("server-name-ambiguous");

    const conflict = resolveFleetMentions(
      parseFleetMentions("@db-primary#source @db-primary#replica").mentions,
      profiles,
      groups,
      [tab("primary")],
    );
    expect(conflict.errors[0]?.code).toBe("target-role-conflict");
  });

  it("requires at least two exact targets for fleet execution", () => {
    const result = resolveFleetMentions(
      parseFleetMentions("@db-primary").mentions,
      profiles,
      groups,
      [tab("primary")],
    );
    expect(result.errors[0]?.code).toBe("too-few-targets");
  });
});

