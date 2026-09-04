import { useTranslation } from "react-i18next";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";
import type { LogSource } from "../../types/infrastructure";

const logSources = ["system", "auth", "nginx-access", "nginx-error", "docker", "pm2", "service"] as const satisfies readonly LogSource[];

export function LogSourceSelect({ value, onValueChange }: { value: LogSource; onValueChange: (value: LogSource) => void }) {
  const { t } = useTranslation();
  return (
    <Select value={value} onValueChange={(next) => {
      const source = logSources.find((candidate) => candidate === next);
      if (source) onValueChange(source);
    }}>
      <SelectTrigger aria-label={t("operations.logSource")}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {logSources.map((source) => <SelectItem key={source} value={source}>{t(`operations.log.${source}`)}</SelectItem>)}
      </SelectContent>
    </Select>
  );
}
