import { ConfigurationPanel } from "@/components/portal/configuration-panel";
import { getDictionary, type Locale } from "@/lib/i18n";

export function AdminStatus({ locale, status }: { locale: Locale; status: "configuration" | "forbidden" | "unavailable" }) {
  const t = getDictionary(locale);
  if (status === "forbidden") return <ConfigurationPanel title={t.admin.forbiddenTitle} description={t.admin.forbiddenDescription} />;
  if (status === "unavailable") return <ConfigurationPanel title={t.admin.migrationTitle} description={t.admin.migrationDescription} />;
  return <ConfigurationPanel title={t.common.configuration} description={t.common.configurationDescription} />;
}
