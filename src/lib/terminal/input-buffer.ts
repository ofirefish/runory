type SendInput = (sessionId: string, bytes: Uint8Array) => Promise<void>;

export class TerminalInputBuffer {
  private readonly encoder = new TextEncoder();
  private readonly chunks: Uint8Array[] = [];
  private bufferedBytes = 0;
  private targetSessionId: string | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private sending = Promise.resolve();

  constructor(
    private readonly send: SendInput,
    private readonly onError: (error: unknown) => void,
    private readonly delayMs = 8,
    private readonly maxChunkBytes = 16 * 1024,
  ) {}

  push(sessionId: string, value: string) {
    if (!value) return;
    if (this.targetSessionId && this.targetSessionId !== sessionId) this.clear();
    this.targetSessionId = sessionId;
    let remaining = this.encoder.encode(value);
    while (remaining.length > 0) {
      const available = this.maxChunkBytes - this.bufferedBytes;
      const part = remaining.slice(0, available);
      this.chunks.push(part);
      this.bufferedBytes += part.length;
      remaining = remaining.slice(part.length);
      if (this.bufferedBytes === this.maxChunkBytes) this.flush();
    }
    if (this.bufferedBytes > 0 && !this.timer) {
      this.timer = setTimeout(() => this.flush(), this.delayMs);
    }
  }

  clear() {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    this.chunks.length = 0;
    this.bufferedBytes = 0;
    this.targetSessionId = null;
  }

  dispose() {
    this.clear();
  }

  private flush() {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    const sessionId = this.targetSessionId;
    if (!sessionId || this.bufferedBytes === 0) return;
    const bytes = new Uint8Array(this.bufferedBytes);
    let offset = 0;
    for (const chunk of this.chunks) {
      bytes.set(chunk, offset);
      offset += chunk.length;
    }
    this.chunks.length = 0;
    this.bufferedBytes = 0;
    this.sending = this.sending
      .then(() => this.send(sessionId, bytes))
      .catch((error: unknown) => this.onError(error));
  }
}
