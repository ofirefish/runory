import { readFileSync } from "node:fs";
import { parse } from "postcss";
import { describe, expect, it } from "vitest";

const stylesheet = parse(readFileSync("src/styles.css", "utf8"));

function declaration(selector: string, property: string) {
  let value: string | undefined;
  stylesheet.walkRules(selector, (rule) => {
    rule.walkDecls(property, (decl) => { value = decl.value; });
  });
  return value;
}

describe("workspace tab corner clipping", () => {
  it("reserves both outer corners inside the horizontal scrollport", () => {
    const cornerWidth = declaration(".workspace-tab.active::before,.workspace-tab.active::after", "width");
    expect(cornerWidth).toBe("12px");
    expect(declaration(".workspace-tab-strip", "padding-inline")).toBe(cornerWidth);
    expect(declaration(".workspace-tab-strip", "scroll-padding-inline")).toBe(cornerWidth);
    expect(declaration(".workspace-tab.active::before", "left")).toBe(`-${cornerWidth}`);
    expect(declaration(".workspace-tab.active::after", "right")).toBe(`-${cornerWidth}`);
    expect(declaration(".workspace-tab-strip", "overflow-x")).toBe("auto");
  });
});
