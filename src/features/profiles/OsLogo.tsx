import type { CSSProperties } from "react";
import type { OsDistribution } from "../../types/domain";
import type { SessionState } from "../../types/session";
import { cn } from "../../lib/utils";
import { osLogoDictionary } from "./os-logo-data";

export function OsLogo({ distribution, state, statusLabel, plain = false }: { distribution: OsDistribution; state: SessionState; statusLabel: string; plain?: boolean }) {
  const logo = osLogoDictionary[distribution] ?? osLogoDictionary.linux;
  const title = `${logo.label} · ${statusLabel}`;
  return <span className={cn("host-os-logo", plain && "plain")} style={{ "--os-logo-color": logo.color } as CSSProperties} aria-label={title} title={title}>
    <span className="host-os-glyph" style={{ "--os-logo-mask": `url("${logo.asset}")` } as CSSProperties} aria-hidden="true" />
    {!plain && <i className={cn("host-os-status", `status-${state}`)} aria-hidden="true" />}
  </span>;
}
