import { describe, expect, it } from "vitest";
import { filterProfiles } from "./search";
import type { HostGroup, ServerProfile } from "../../types/domain";

const group: HostGroup = { id: "group-1", name: "Production", sortOrder: 0, collapsed: false, createdAt: "1", updatedAt: "1" };
const profiles: ServerProfile[] = [
  { id: "profile-1", name: "API", host: "10.0.0.1", port: 22, username: "root", groupId: group.id, authMethod: "password", sortOrder: 0, createdAt: "1", updatedAt: "1" },
  { id: "profile-2", name: "Staging", host: "staging.example.com", port: 22, username: "deploy", groupId: null, authMethod: "password", sortOrder: 1, createdAt: "1", updatedAt: "1" },
];

describe("filterProfiles", () => {
  it("matches name, host, username, and group name", () => {
    expect(filterProfiles([group], profiles, "production")).toEqual([profiles[0]]);
    expect(filterProfiles([group], profiles, "example.com")).toEqual([profiles[1]]);
    expect(filterProfiles([group], profiles, "ROOT")).toEqual([profiles[0]]);
  });
});
