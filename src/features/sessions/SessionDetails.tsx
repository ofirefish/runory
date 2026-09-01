import { useTranslation } from "react-i18next";
import type { ServerProfile } from "../../types/domain";
import type { SessionState } from "../../types/session";

export function SessionDetails({ profile, state, sessionId }: { profile: ServerProfile | null; state: SessionState; sessionId: string | null }) {
  const { t } = useTranslation();
  if (!profile) return null;
  const rows = [
    [t("connection.host"), profile.host],
    [t("connection.port"), String(profile.port)],
    [t("connection.username"), profile.username],
    [t("profile.authMethod"), t(profile.authMethod === "password" ? "profile.password" : "profile.privateKey")],
    [t("details.status"), t(`status.${state}`)],
    [t("details.sessionId"), sessionId ?? "—"],
  ];
  return <section className="h-full overflow-auto bg-[hsl(var(--surface))] p-6" aria-label={t("details.title")}>
    <h2 className="mb-5 text-lg font-semibold">{t("details.title")}</h2>
    <dl className="max-w-2xl divide-y rounded-lg border">
      {rows.map(([label, value]) => <div key={label} className="grid grid-cols-[minmax(9rem,0.35fr)_1fr] gap-4 px-4 py-3 text-sm"><dt className="text-[hsl(var(--secondary))]">{label}</dt><dd className="break-all font-mono">{value}</dd></div>)}
    </dl>
  </section>;
}
