import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "./button";
import { DialogShell } from "./dialog-shell";

export function ConfirmDialog({ title, description, onConfirm, onClose }: { title: string; description: string; onConfirm: () => Promise<void>; onClose: () => void }) {
  const { t } = useTranslation(); const [busy, setBusy] = useState(false); const [failure, setFailure] = useState(false);
  const confirm = async () => { setBusy(true); setFailure(false); try { await onConfirm(); onClose(); } catch { setFailure(true); setBusy(false); } };
  return <DialogShell title={title} onClose={onClose}><p className="text-sm text-[hsl(var(--secondary))]">{description}</p>{failure && <p className="mt-3 text-sm text-red-500">{t("errors.deleteFailed")}</p>}<div className="mt-5 flex justify-end gap-2"><Button variant="ghost" onClick={onClose}>{t("common.cancel")}</Button><Button variant="danger" disabled={busy} onClick={() => void confirm()}>{t("common.delete")}</Button></div></DialogShell>;
}
