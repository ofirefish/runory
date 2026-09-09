import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { sessionTunnelImpact } from "../../lib/tauri/tunnels";
import { useSessionStore } from "../../stores/session-store";
import type { TunnelRule } from "../../types/tunnels";

export function useTunnelDisconnect() {
  const { t } = useTranslation();
  const [pending, setPending] = useState<{ affected: TunnelRule[]; perform: () => Promise<void> } | null>(null);
  const [failure, setFailure] = useState(false);
  const [busy, setBusy] = useState(false);
  const checking = useRef(false);
  const request = async (tabId: string, perform: () => Promise<void>) => {
    if (checking.current) return;
    checking.current = true;
    setFailure(false);
    const original = useSessionStore.getState().tabs.find((tab) => tab.id === tabId);
    const boundPerform = async () => {
      const current = useSessionStore.getState().tabs.find((tab) => tab.id === tabId);
      if (!current || current.sessionId !== original?.sessionId || current.connectionAttemptId !== original?.connectionAttemptId) return;
      await perform();
    };
    try {
      const affected = original?.sessionId ? await sessionTunnelImpact(original.sessionId) : [];
      if (affected.length) setPending({ affected, perform: boundPerform });
      else await boundPerform();
    } catch { setFailure(true); }
    finally { checking.current = false; }
  };
  const confirm = async () => {
    if (!pending || busy) return;
    setBusy(true);
    try { await pending.perform(); setPending(null); }
    catch { setFailure(true); }
    finally { setBusy(false); }
  };
  const dialog = (pending || failure) && <DialogShell title={t("tunnels.disconnectTitle")} onClose={() => { if (!busy) { setPending(null); setFailure(false); } }}>
    {failure && <p role="alert">{t("tunnels.disconnectFailed")}</p>}
    {pending && <><p>{t("tunnels.disconnectHint")}</p><ul className="my-3 list-disc pl-5">{pending.affected.map((rule) => <li key={rule.id}>{rule.name} · <code>127.0.0.1:{rule.localPort}</code></li>)}</ul></>}
    <div className="mt-5 flex justify-end gap-2"><Button variant="ghost" disabled={busy} onClick={() => { setPending(null); setFailure(false); }}>{t("common.cancel")}</Button>{pending && <Button variant="danger" disabled={busy} onClick={() => void confirm()}>{t("tunnels.disconnectConfirm")}</Button>}</div>
  </DialogShell>;
  return { request, dialog };
}
