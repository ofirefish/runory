import { describe, expect, it } from "vitest";
import { moveId } from "./reorder";

describe("moveId", () => {
  it("moves an item one position without changing the input", () => {
    const input = ["one", "two", "three"];
    expect(moveId(input, "two", -1)).toEqual(["two", "one", "three"]);
    expect(input).toEqual(["one", "two", "three"]);
  });

  it("keeps boundary and unknown items unchanged", () => {
    const input = ["one", "two"];
    expect(moveId(input, "one", -1)).toBe(input);
    expect(moveId(input, "two", 1)).toBe(input);
    expect(moveId(input, "missing", 1)).toBe(input);
  });
});
