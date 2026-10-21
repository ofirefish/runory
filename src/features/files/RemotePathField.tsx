import { FolderOpen } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import { RemoteFolderPickerDialog } from "./RemoteFolderPickerDialog";

export function RemotePathField({
  label,
  value,
  onChange,
  sessionId,
  placeholder,
  disabled,
  allowCreateDirectory = true,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  sessionId: string | null;
  placeholder?: string;
  disabled?: boolean;
  allowCreateDirectory?: boolean;
}) {
  const { t } = useTranslation();
  const id = useId();
  const [open, setOpen] = useState(false);
  const browseDisabled = disabled || !sessionId;

  return (
    <div className="grid min-w-0 gap-1.5">
      <Label htmlFor={id}>{label}</Label>
      <div className="flex min-w-0 gap-2">
        <Input
          id={id}
          value={value}
          placeholder={placeholder}
          disabled={disabled}
          className="min-w-0 flex-1 font-mono text-foreground focus:ring-primary"
          onChange={(event) => onChange(event.target.value)}
        />
        <Button
          type="button"
          variant="secondary"
          size="icon"
          className="shrink-0"
          disabled={browseDisabled}
          title={sessionId ? t("remoteFolderPicker.browse") : t("deployment.connectRequiredAction")}
          aria-label={t("remoteFolderPicker.browse")}
          onClick={() => setOpen(true)}
        >
          <FolderOpen size={16} />
        </Button>
      </div>
      {open && sessionId && (
        <RemoteFolderPickerDialog
          sessionId={sessionId}
          initialPath={value}
          allowCreateDirectory={allowCreateDirectory}
          onClose={() => setOpen(false)}
          onSelect={onChange}
        />
      )}
    </div>
  );
}
