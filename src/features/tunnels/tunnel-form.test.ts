import { describe, expect, it } from "vitest";
import { profileComboboxGroups, tunnelSchema } from "./tunnel-form";
import { localEndpoint, targetEndpoint } from "../../types/tunnels";

const values = { name: "Database", profileId: "profile", targetHost: "db.internal", targetPort: 5432, localPort: 15432 };
describe("tunnel rule form", () => {
  it("accepts remote DNS and unbracketed IPv6 without resolving locally", () => {
    for (const targetHost of ["db.internal", "tunnel-target", "127.0.0.1", "::1", "db.internal."]) expect(tunnelSchema.safeParse({ ...values, targetHost }).success).toBe(true);
  });
  it("rejects malformed endpoints and invalid ports", () => {
    for (const targetHost of ["", "http://db", "db:5432", "user@db", "db/path", "a..b", "-db", "db;id"]) expect(tunnelSchema.safeParse({ ...values, targetHost }).success).toBe(false);
    for (const localPort of [0, -1, 65536, 123.5, NaN]) expect(tunnelSchema.safeParse({ ...values, localPort }).success).toBe(false);
    expect(tunnelSchema.safeParse({ ...values, profileId: "" }).success).toBe(false);
  });
  it("displays the fixed loopback endpoint and brackets IPv6 destinations", () => {
    const rule = { ...values, id: "rule", targetHost: "::1" };
    expect(localEndpoint(rule)).toBe("127.0.0.1:15432");
    expect(targetEndpoint(rule)).toBe("[::1]:5432");
  });
  it("groups SSH servers by catalog group and keeps ungrouped servers last", () => {
    const groups = [
      { id: "prod", name: "Production", sortOrder: 0, collapsed: false, createdAt: "", updatedAt: "" },
      { id: "stag", name: "Staging", sortOrder: 1, collapsed: false, createdAt: "", updatedAt: "" },
    ];
    const profiles = [
      { id: "alpha", name: "Alpha", host: "10.0.0.1", port: 22, username: "ops", groupId: "prod", sortOrder: 0, authMethod: "password" as const, connectionRoute: { type: "direct" as const }, createdAt: "", updatedAt: "" },
      { id: "beta", name: "Beta", host: "10.0.0.2", port: 22, username: "ops", groupId: "stag", sortOrder: 0, authMethod: "password" as const, connectionRoute: { type: "direct" as const }, createdAt: "", updatedAt: "" },
      { id: "solo", name: "Solo", host: "10.0.0.3", port: 22, username: "ops", groupId: null, sortOrder: 0, authMethod: "password" as const, connectionRoute: { type: "direct" as const }, createdAt: "", updatedAt: "" },
    ];
    expect(profileComboboxGroups(groups, profiles, "Ungrouped").map((group) => [group.label, group.options.map((option) => option.label)])).toEqual([
      ["Production", ["Alpha"]],
      ["Staging", ["Beta"]],
      ["Ungrouped", ["Solo"]],
    ]);
  });
});
