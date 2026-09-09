import { describe, expect, it } from "vitest";
import { tunnelSchema } from "./tunnel-form";
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
});
