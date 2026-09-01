import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../ui/button";
import { DialogShell } from "../ui/dialog-shell";
import { Input } from "../ui/input";

export function RenameWorkspaceTabDialog({ initialTitle, onClose, onSave }: {
  initialTitle: string;
  onClose: () => void;
  onSave: (title: string) => void;
}) {
  const { t } = useTranslation();
  const [title, setTitle] = useState(initialTitle);
  const normalizedTitle = title.trim();
  const invalid = normalizedTitle.length === 0 || normalizedTitle.length > 50;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!invalid) onSave(normalizedTitle);
  };

  return <DialogShell title={t("terminal.renameTabTitle")} onClose={onClose}>
    <form onSubmit={submit}>
      <label className="block text-sm font-medium">{t("terminal.tabName")}
        <Input className="mt-1" autoFocus maxLength={50} value={title} aria-invalid={invalid} onChange={(event) => setTitle(event.target.value)} onFocus={(event) => event.currentTarget.select()} />
      </label>
      {invalid && <p className="mt-1 text-xs text-red-500">{t("validation.tabName")}</p>}
      <div className="mt-5 flex justify-end gap-2">
        <Button type="button" variant="ghost" onClick={onClose}>{t("common.cancel")}</Button>
        <Button type="submit" disabled={invalid}>{t("common.save")}</Button>
      </div>
    </form>
  </DialogShell>;
}
