import { afterEach, describe, expect, it, vi } from "vitest";
import { TerminalInputBuffer } from "./input-buffer";

afterEach(() => vi.useRealTimers());

describe("TerminalInputBuffer", () => {
  it("coalesces keyboard input during the short buffer window", async () => {
    vi.useFakeTimers();
    const sent: string[] = [];
    const buffer = new TerminalInputBuffer(async (sessionId, bytes) => {
      sent.push(`${sessionId}:${new TextDecoder().decode(bytes)}`);
    }, () => undefined);

    buffer.push("session-one", "a");
    buffer.push("session-one", "中");
    await vi.advanceTimersByTimeAsync(8);
    await Promise.resolve();

    expect(sent).toEqual(["session-one:a中"]);
  });

  it("chunks large paste input and preserves send order", async () => {
    const sent: number[][] = [];
    const buffer = new TerminalInputBuffer(async (_sessionId, bytes) => {
      sent.push(Array.from(bytes));
    }, () => undefined, 8, 4);

    buffer.push("session-one", "abcdefgh");
    await vi.waitFor(() => {
      expect(sent.map((bytes) => new TextDecoder().decode(new Uint8Array(bytes)))).toEqual(["abcd", "efgh"]);
    });
  });

  it("does not retarget pending input to a reconnected session", async () => {
    vi.useFakeTimers();
    const targets: string[] = [];
    const buffer = new TerminalInputBuffer(async (sessionId) => {
      targets.push(sessionId);
    }, () => undefined);

    buffer.push("old-session", "old");
    buffer.push("new-session", "new");
    await vi.advanceTimersByTimeAsync(8);
    await Promise.resolve();

    expect(targets).toEqual(["new-session"]);
  });
});
