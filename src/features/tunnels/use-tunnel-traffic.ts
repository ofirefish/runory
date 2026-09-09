import { useEffect, useRef, useState } from "react";
import type { TunnelStatus } from "../../types/tunnels";

const IDLE = { sending: false, receiving: false };
// Counters arrive on the existing 2-second metadata poll, not per packet.
const ACTIVITY_WINDOW_MS = 2400;

export function useTunnelTraffic(ruleId: string, status: TunnelStatus) {
  const { state, sessionId, startedAt, bytesSent, bytesReceived } = status;
  const previous = useRef<{ ruleId: string; sessionId: string | null; startedAt: number | null; state: string; bytesSent: number; bytesReceived: number } | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const [traffic, setTraffic] = useState(IDLE);

  useEffect(() => {
    const before = previous.current;
    previous.current = { ruleId, sessionId, startedAt, state, bytesSent, bytesReceived };
    const sameRun = before?.ruleId === ruleId && before.sessionId === sessionId && before.startedAt === startedAt && before.state === "running" && state === "running";
    if (!sameRun || bytesSent < before.bytesSent || bytesReceived < before.bytesReceived) {
      clearTimeout(timer.current);
      setTraffic(IDLE);
      return;
    }
    const sending = bytesSent > before.bytesSent;
    const receiving = bytesReceived > before.bytesReceived;
    if (sending || receiving) {
      clearTimeout(timer.current);
      setTraffic({ sending, receiving });
      timer.current = setTimeout(() => setTraffic(IDLE), ACTIVITY_WINDOW_MS);
    }
  }, [ruleId, state, sessionId, startedAt, bytesSent, bytesReceived]);

  useEffect(() => () => clearTimeout(timer.current), []);
  return state === "running" ? traffic : IDLE;
}
