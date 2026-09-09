import { AlertTriangle } from "lucide-react";

export function ConfigurationPanel({ title, description }: { title: string; description: string }) {
  return (
    <div className="status-panel" role="status">
      <AlertTriangle size={22} />
      <div><h1>{title}</h1><p>{description}</p></div>
    </div>
  );
}
