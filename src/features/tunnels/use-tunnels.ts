import { useCallback, useEffect, useRef, useState } from "react";
import { appErrorCode } from "../../lib/app-error";
import { listTunnels } from "../../lib/tauri/tunnels";
import type { TunnelView } from "../../types/tunnels";

export function useTunnels() {
  const [items, setItems] = useState<TunnelView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const requestId = useRef(0);
  const invalidate = useCallback(() => { requestId.current++; }, []);
  const refresh = useCallback(async () => {
    const id = ++requestId.current;
    try {
      const result = await listTunnels();
      if (id === requestId.current) { setItems(result); setError(null); }
    } catch (failure) { if (id === requestId.current) setError(appErrorCode(failure)); }
    finally { if (id === requestId.current) setLoading(false); }
  }, []);
  useEffect(() => {
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => { await refresh(); if (!disposed) timer = setTimeout(() => void poll(), 2000); };
    void poll();
    return () => { disposed = true; clearTimeout(timer); invalidate(); };
  }, [invalidate, refresh]);
  return { items, error, loading, refresh };
}
