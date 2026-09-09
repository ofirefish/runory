import { useTranslation } from "react-i18next";
import { DialogShell } from "../../components/ui/dialog-shell";
import { BillingPanel } from "./BillingPanel";

export function PricingDialog({ onClose, onOpenAuth }: { onClose: () => void; onOpenAuth: () => void }) {
  const { t } = useTranslation();
  return <DialogShell title={t("pricing.title")} onClose={onClose} size="pricing" panelClassName="pricing-dialog-panel" contentClassName="pricing-dialog-content">
    <BillingPanel embedded onOpenAccount={() => { onClose(); onOpenAuth(); }} />
  </DialogShell>;
}
