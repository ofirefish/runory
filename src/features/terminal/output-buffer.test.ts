import { describe, expect, it } from "vitest";
import { TerminalOutputBuffer, toUint8Array } from "./output-buffer";

describe("terminal output buffer", () => {
  it("preserves ESC bytes when normalizing IPC arrays", () => {
    const bytes = toUint8Array([0x1b, 0x5b, 0x33, 0x31, 0x6d, 0x52]);
    expect([...bytes]).toEqual([27, 91, 51, 49, 109, 82]);
  });

  it("buffers chunks until a sink attaches then flushes in order", () => {
    const buffer = new TerminalOutputBuffer();
    const received: number[] = [];
    buffer.write(Uint8Array.from([0x1b, 0x5b, 0x33, 0x31, 0x6d]));
    buffer.write(Uint8Array.from([0x52, 0x45, 0x44]));
    buffer.attach((chunk) => received.push(...chunk));
    expect(received).toEqual([27, 91, 51, 49, 109, 82, 69, 68]);
    buffer.write(Uint8Array.from([0x1b, 0x5b, 0x30, 0x6d]));
    expect(received.slice(-4)).toEqual([27, 91, 48, 109]);
  });
});
