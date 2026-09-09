import { Check, Circle, LoaderCircle, Monitor, Network, Server } from "lucide-react";
import { useTranslation } from "react-i18next";

export type JumpConnectionProgressStage = "jump" | "route" | "connect";

export function JumpConnectionProgress({ stage, testing }: {
  stage: JumpConnectionProgressStage;
  testing: boolean;
}) {
  const { t } = useTranslation();
  const steps = ["jump", "route", "connect"] as const;
  const activeIndex = steps.indexOf(stage);

  return <section className="connection-attempt jump-connection-attempt" aria-label={t("connection.jumpProgress.title")}>
    <div className="jump-route-visual" aria-hidden="true">
      <span className="connection-route-endpoint"><Monitor size={20} strokeWidth={1.5} /></span>
      <span className="connection-route-line"><i /></span>
      <span className="jump-route-node"><Network size={18} strokeWidth={1.6} /></span>
      <span className="connection-route-line"><i /></span>
      <span className="connection-route-endpoint remote"><Server size={20} strokeWidth={1.5} /></span>
    </div>
    <div className="connection-attempt-heading" role="status" aria-live="polite">
      <h3>{t(testing ? "connection.jumpProgress.testing" : "connection.jumpProgress.connecting")}</h3>
      <p>{t(`connection.jumpProgress.${stage}Hint`)}</p>
    </div>
    <ol className="connection-attempt-steps">
      {steps.map((step, index) => {
        const completed = index < activeIndex;
        const active = step === stage;
        const Icon = completed ? Check : active ? LoaderCircle : Circle;
        return <li key={step} data-state={completed ? "complete" : active ? "active" : "pending"} aria-current={active ? "step" : undefined}>
          <span className="connection-step-icon"><Icon size={15} aria-hidden="true" /></span>
          <span>{t(`connection.jumpProgress.${step}`)}</span>
          <small>{completed ? t("connection.progress.verified") : active ? t("connection.progress.inProgress") : String(index + 1).padStart(2, "0")}</small>
        </li>;
      })}
    </ol>
  </section>;
}
