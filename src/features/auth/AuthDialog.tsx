import { useTranslation } from "react-i18next";
import { DialogShell } from "../../components/ui/dialog-shell";
import { AccountSettings } from "../settings/AccountSettings";

export function AuthDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  return <DialogShell title={t("cloud.signInTitle")} onClose={onClose} size="form" contentClassName="auth-dialog-content">
    <AccountSettings onAuthenticated={onClose} />
  </DialogShell>;
}
