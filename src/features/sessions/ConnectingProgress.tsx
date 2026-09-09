import { LoaderCircle, Server } from "lucide-react";
import "./connecting-progress.css";

/** Indeterminate connecting indicator for SSH connection dialogs. */
export function ConnectingProgress({ label }: { label: string }) {
  return (
    <div className="connection-progress" role="status" aria-live="polite">
      <div className="connection-progress-visual" aria-hidden="true">
        <span className="connection-progress-node local" />
        <span className="connection-progress-link">
          <i />
          <i />
          <i />
        </span>
        <span className="connection-progress-node remote"><Server size={12} strokeWidth={2.25} /></span>
      </div>
      <p className="connection-progress-copy">
        <LoaderCircle className="connection-progress-spinner" size={14} aria-hidden="true" />
        <span>{label}</span>
      </p>
    </div>
  );
}
