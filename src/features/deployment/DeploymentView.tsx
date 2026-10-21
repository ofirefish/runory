import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { listDeploymentApps } from "../../lib/tauri/infrastructure";
import type { DeploymentApp } from "../../types/infrastructure";
import { cn } from "../../lib/utils";
import { GoProjectPanel } from "./GoProjectPanel";
import { HtmlProjectPanel } from "./HtmlProjectPanel";
import { JavaProjectPanel } from "./JavaProjectPanel";
import { NodeProjectPanel } from "./NodeProjectPanel";
import { PhpProjectPanel } from "./PhpProjectPanel";
import { ProxyProjectPanel } from "./ProxyProjectPanel";
import { PythonProjectPanel } from "./PythonProjectPanel";
import { PluginWorkspaceSection } from "../plugins/workspace/PluginWorkspaceSection";

type ProjectKind = "php" | "java" | "node" | "go" | "python" | "reverseProxy" | "html";

const projectKinds: ProjectKind[] = ["php", "java", "node", "go", "python", "reverseProxy", "html"];

export function DeploymentView({ sessionId, profileId }: { sessionId: string | null; profileId: string }) {
  const { t } = useTranslation();
  const [projectKind, setProjectKind] = useState<ProjectKind>("php");
  const [apps, setApps] = useState<DeploymentApp[]>([]);

  const refreshApps = useCallback(async () => {
    try {
      setApps(await listDeploymentApps(profileId));
    } catch {
      setApps([]);
    }
  }, [profileId]);

  useEffect(() => {
    void refreshApps();
  }, [refreshApps]);

  const visibleApps = apps.filter((app) => app.projectKind === projectKind);
  const panelKey = `${profileId}:${sessionId ?? "offline"}`;
  const panelProps = { sessionId, profileId, apps: visibleApps, onChanged: refreshApps };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <section className="flex min-h-0 flex-1 flex-col bg-[hsl(var(--surface))]" aria-label={t("deployment.title")}>
        <nav
          className="flex h-11 shrink-0 items-center gap-1 overflow-x-auto border-b px-3"
          role="tablist"
          aria-label={t("deployment.projectKinds")}
          aria-orientation="horizontal"
        >
          {projectKinds.map((kind) => (
            <Button
              key={kind}
              type="button"
              role="tab"
              id={`deployment-project-${kind}`}
              variant="ghost"
              size="sm"
              className={cn(
                "shrink-0 px-3 font-medium",
                projectKind === kind
                  ? "text-[hsl(var(--primary))] hover:text-[hsl(var(--primary))]"
                  : "text-[hsl(var(--secondary))] hover:text-[hsl(var(--foreground))]",
              )}
              aria-selected={projectKind === kind}
              aria-controls="deployment-project-panel"
              tabIndex={projectKind === kind ? 0 : -1}
              onClick={() => setProjectKind(kind)}
            >
              {t(`deployment.project.${kind}`)}
            </Button>
          ))}
        </nav>
        <div id="deployment-project-panel" role="tabpanel" aria-labelledby={`deployment-project-${projectKind}`} className="flex min-h-0 flex-1">
          {projectKind === "php" && <PhpProjectPanel key={panelKey} {...panelProps} />}
          {projectKind === "java" && <JavaProjectPanel key={panelKey} {...panelProps} />}
          {projectKind === "node" && <NodeProjectPanel key={panelKey} {...panelProps} />}
          {projectKind === "go" && <GoProjectPanel key={panelKey} {...panelProps} />}
          {projectKind === "python" && <PythonProjectPanel key={panelKey} {...panelProps} />}
          {projectKind === "reverseProxy" && <ProxyProjectPanel key={panelKey} {...panelProps} />}
          {projectKind === "html" && <HtmlProjectPanel key={panelKey} {...panelProps} />}
        </div>
      </section>
      <div className="max-h-56 shrink-0 overflow-auto bg-[hsl(var(--surface))] px-4 pb-3">
        <PluginWorkspaceSection
          category="deployment"
          titleKey="plugins.workspace.section.deployment"
          hostId={profileId}
          sessionId={sessionId}
        />
      </div>
    </div>
  );
}
