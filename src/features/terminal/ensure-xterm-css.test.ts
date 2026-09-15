// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";

describe("ensureXtermCssInjected", () => {
  beforeEach(() => {
    document.getElementById("runory-xterm-css")?.remove();
  });

  it("injects xterm CSS into document head once", async () => {
    const { ensureXtermCssInjected } = await import("./ensure-xterm-css");
    ensureXtermCssInjected();
    ensureXtermCssInjected();
    const styles = document.querySelectorAll("#runory-xterm-css");
    expect(styles).toHaveLength(1);
    expect(styles[0]?.textContent ?? "").toContain(".xterm");
    expect(styles[0]?.textContent ?? "").toContain("xterm-accessibility");
    expect(styles[0]?.textContent ?? "").toContain("display: none");
  });
});
