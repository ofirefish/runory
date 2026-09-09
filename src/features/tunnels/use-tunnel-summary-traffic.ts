import { useEffect, useRef, useState } from "react";
import type { TunnelView } from "../../types/tunnels";

const IDLE = { sent: 0, received: 0 };

export function useTunnelSummaryTraffic(items: TunnelView[], unavailable: boolean) {
  const previous = useRef<{ at: number; items: TunnelView[] } | null>(null);
  const [traffic, setTraffic] = useState(IDLE);
  useEffect(() => {
    if (unavailable) { previous.current = null; setTraffic(IDLE); return; }
    const at = performance.now();
    const before = previous.current;
    previous.current = { at, items };
    let sent = 0;
    let received = 0;
    const elapsed = before ? (at - before.at) / 1000 : 0;
    if (before && elapsed > 0) {
      for (const { rule, status } of items) {
        const old = before.items.find((item) => item.rule.id === rule.id)?.status;
        // Compare each listener generation separately: starts, stops and counter resets are not traffic.
        if (!old || old.state !== "running" || status.state !== "running" || old.sessionId !== status.sessionId || old.startedAt !== status.startedAt || status.bytesSent < old.bytesSent || status.bytesReceived < old.bytesReceived) continue;
        sent += (status.bytesSent - old.bytesSent) / elapsed;
        received += (status.bytesReceived - old.bytesReceived) / elapsed;
      }
    }
    setTraffic({ sent, received });
    // Never leave an activity indicator running if a later metadata request stalls.
    const timer = setTimeout(() => setTraffic(IDLE), 2400);
    return () => clearTimeout(timer);
  }, [items, unavailable]);
  return unavailable ? IDLE : traffic;
}
