import { useTranslation } from "react-i18next";
import { PluginsHub } from "./components/PluginsHub";
import "./plugins.css";

export function PluginsPage() {
  const { t } = useTranslation();
  return (
    <main className="plugins-page" aria-label={t("plugins.page.title")}>
      <PluginsHub variant="page" initialTab="discover" />
    </main>
  );
}
