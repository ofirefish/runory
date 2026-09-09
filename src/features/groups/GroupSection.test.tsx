// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GroupSection } from "./GroupSection";

const labels = { add: "Add", edit: "Edit", delete: "Delete", moveUp: "Up", moveDown: "Down", more: "More" };
const add = vi.fn();
let container: HTMLDivElement;
let root: Root;
const menu = (index = 0) => container.querySelectorAll<HTMLDetailsElement>("details")[index];
async function open(index = 0) {
  await act(async () => { menu(index).querySelector("summary")?.click(); });
  expect(menu(index).open).toBe(true);
}
beforeEach(async () => {
  vi.clearAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  container = document.createElement("div"); document.body.append(container); root = createRoot(container);
  await act(async () => {
    root.render(<><GroupSection name="Production" count={1} labels={labels} onAdd={add}><button className="host-fixture" onPointerDown={(event) => event.stopPropagation()} onClick={(event) => event.stopPropagation()}>Host</button></GroupSection><GroupSection name="Staging" count={0} labels={labels} onAdd={add}>{null}</GroupSection><button className="outside">Outside</button></>);
  });
});
afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove(); vi.unstubAllGlobals();
});

describe("group menu dismissal", () => {
  it("closes when clicking blank space anywhere outside the menu", async () => {
    await open();
    await act(async () => { document.body.dispatchEvent(new Event("pointerdown", { bubbles: true })); });
    expect(menu().open).toBe(false);
  });

  it("closes on keyboard-generated external clicks", async () => {
    await open();
    await act(async () => { container.querySelector<HTMLButtonElement>(".outside")?.click(); });
    expect(menu().open).toBe(false);
  });

  it("closes even when the outside control stops propagation", async () => {
    for (const eventType of ["pointerdown", "click"]) {
      await open();
      await act(async () => { container.querySelector(".host-fixture")?.dispatchEvent(new Event(eventType, { bubbles: true })); });
      expect(menu().open).toBe(false);
    }
  });

  it("closes the previous group menu when opening another", async () => {
    await open(); await open(1);
    expect(menu().open).toBe(false);
    expect(menu(1).open).toBe(true);
  });

  it("allows menu actions to run before closing and preserves summary toggling", async () => {
    await open();
    const action = menu().querySelector<HTMLButtonElement>("[role='menuitem']")!;
    await act(async () => { action.dispatchEvent(new Event("pointerdown", { bubbles: true })); });
    expect(menu().open).toBe(true);
    await act(async () => { action.click(); });
    expect(add).toHaveBeenCalledTimes(1);
    expect(menu().open).toBe(false);
    await open();
    await act(async () => { menu().querySelector("summary")?.click(); });
    expect(menu().open).toBe(false);
  });
});
