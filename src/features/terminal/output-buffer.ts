/** Ring buffer for PTY bytes that arrive before xterm has finished opening. */
export class TerminalOutputBuffer {
  private readonly chunks: Uint8Array[] = [];
  private bytes = 0;
  private sink: ((chunk: Uint8Array) => void) | null = null;

  constructor(private readonly maxBytes = 256 * 1024) {}

  write(chunk: Uint8Array) {
    if (this.sink) {
      this.sink(chunk);
      return;
    }
    this.chunks.push(chunk);
    this.bytes += chunk.length;
    while (this.bytes > this.maxBytes && this.chunks.length > 0) {
      const dropped = this.chunks.shift();
      if (dropped) this.bytes -= dropped.length;
    }
  }

  /** Attach the live xterm writer and flush any buffered output in order. */
  attach(sink: (chunk: Uint8Array) => void) {
    this.sink = sink;
    for (const chunk of this.chunks) sink(chunk);
    this.chunks.length = 0;
    this.bytes = 0;
  }

  detach() {
    this.sink = null;
    this.chunks.length = 0;
    this.bytes = 0;
  }
}

export function toUint8Array(bytes: number[] | Uint8Array): Uint8Array {
  if (bytes instanceof Uint8Array) return bytes;
  const out = new Uint8Array(bytes.length);
  for (let i = 0; i < bytes.length; i += 1) out[i] = Number(bytes[i]) & 0xff;
  return out;
}
