import { Check, Circle, LoaderCircle, Monitor, Server, ShieldCheck } from "lucide-react";
import type { CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import type { OsDistribution } from "../../types/domain";
import { OsLogo } from "../profiles/OsLogo";
import { osLogoDictionary } from "../profiles/os-logo-data";

export type ConnectionProgressStage = "verify" | "connect";

export function ConnectionAttemptProgress({ stage, testing, osDistribution }: {
  stage: ConnectionProgressStage;
  testing: boolean;
  osDistribution?: OsDistribution;
}) {
  const { t } = useTranslation();
  const steps = ["verify", "connect"] as const;
  const osLogo = osDistribution ? osLogoDictionary[osDistribution] : null;
  return <section className="connection-attempt" aria-label={t("connection.progress.title")}>
    <div className="connection-route-visual" aria-hidden="true">
      <span className="connection-route-endpoint"><Monitor size={21} strokeWidth={1.5} /></span>
      <span className="connection-route-line"><i /></span>
      <span className="connection-route-shield"><ShieldCheck size={17} strokeWidth={1.5} /></span>
      <span className="connection-route-line"><i /></span>
      <span
        className={osLogo ? "connection-route-endpoint remote has-os-logo" : "connection-route-endpoint remote"}
        style={osLogo ? { "--os-logo-color": osLogo.color } as CSSProperties : undefined}
      >
        {osDistribution && osLogo
          ? <OsLogo plain distribution={osDistribution} state="idle" statusLabel={osLogo.label} />
          : <Server size={21} strokeWidth={1.5} />}
      </span>
    </div>
    <div className="connection-attempt-heading" role="status" aria-live="polite">
      <h3>{t(testing ? "connection.progress.testing" : "connection.progress.connecting")}</h3>
      <p>{t(stage === "verify" ? "connection.progress.verifyHint" : testing ? "connection.progress.testHint" : "connection.progress.connectHint")}</p>
    </div>
    <ol className="connection-attempt-steps">
      {steps.map((step, index) => {
        const completed = step === "verify" && stage === "connect";
        const active = step === stage;
        const Icon = completed ? Check : active ? LoaderCircle : Circle;
        return <li key={step} data-state={completed ? "complete" : active ? "active" : "pending"} aria-current={active ? "step" : undefined}>
          <span className="connection-step-icon"><Icon size={15} aria-hidden="true" /></span>
          <span>{t(`connection.progress.${step}`)}</span>
          <small>{completed ? t("connection.progress.verified") : active ? t("connection.progress.inProgress") : String(index + 1).padStart(2, "0")}</small>
        </li>;
      })}
    </ol>
  </section>;
}
