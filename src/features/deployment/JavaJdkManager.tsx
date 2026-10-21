import { RefreshCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { installJdk, listJavaRuntimes, setDefaultJdk } from "../../lib/tauri/infrastructure";
import type { JavaRuntime, JdkVersion } from "../../types/infrastructure";
import { DeploymentSelectField as SelectField } from "./DeploymentFields";
import { ConfirmOperation, type Pending } from "./PythonProjectPanel";

const jdkVersions = ["8", "11", "17", "21", "25"] as const satisfies readonly JdkVersion[];

/** Lists remote JDKs, installs an LTS through the system package manager and switches the system default. */
export function JavaJdkManager({ sessionId, onClose }: { sessionId: string | null; onClose: () => void }) {
  const { t } = useTranslation();
  const [runtimes, setRuntimes] = useState<JavaRuntime[]>([]);
  const [loading, setLoading] = useState(false);
  const [version, setVersion] = useState<JdkVersion>("17");
  const [pending, setPending] = useState<Pending | null>(null);
  const [output, setOutput] = useState("");
  const [failed, setFailed] = useState(false);

  const refresh = useCallback(async () => {
    if (!sessionId) return;
    setLoading(true);
    try {
      setRuntimes(await listJavaRuntimes(sessionId));
    } catch {
      setRuntimes([]);
    } finally {
      setLoading(false);
    }
  }, [sessionId]);

  useEffect(() => { void refresh(); }, [refresh]);

  const runConfirmed = (description: string, operation: () => Promise<{ success: boolean; output: string }>) => {
    setPending({
      description,
      run: async () => {
        try {
          const result = await operation();
          setOutput(result.output);
          setFailed(!result.success);
          if (!result.success) throw new Error("jdk operation failed");
        } catch (caught) {
          setFailed(true);
          throw caught;
        } finally {
          await refresh();
        }
      },
    });
  };

  const install = () => {
    if (!sessionId) return;
    runConfirmed(t("deployment.java.jdk.installConfirm", { version }), () => installJdk(sessionId, version));
  };

  const makeDefault = (runtime: JavaRuntime) => {
    if (!sessionId) return;
    runConfirmed(
      t("deployment.java.jdk.defaultConfirm", { binary: runtime.binary }),
      () => setDefaultJdk(sessionId, runtime.binary),
    );
  };

  return (
    <>
      <DialogShell title={t("deployment.java.jdk.title")} onClose={onClose} size="form">
        <div className="grid gap-4">
          {!sessionId && <p className="text-xs text-amber-600">{t("deployment.connectRequiredAction")}</p>}
          <div className="flex items-end gap-2">
            <div className="min-w-0 flex-1">
              <SelectField
                label={t("deployment.java.jdk.installVersion")}
                value={version}
                disabled={!sessionId}
                options={jdkVersions.map((item) => ({ value: item, label: `OpenJDK ${item}` }))}
                onChange={(value) => setVersion(value)}
              />
            </div>
            <Button disabled={!sessionId || Boolean(pending)} onClick={install}>{t("deployment.java.jdk.install")}</Button>
          </div>
          <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.java.jdk.installHint")}</p>
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium">{t("deployment.java.jdk.installed")}</h3>
            <Button variant="ghost" size="icon" disabled={!sessionId || loading} aria-label={t("common.refresh")} onClick={() => void refresh()}>
              <RefreshCw size={16} className={loading ? "animate-spin" : undefined} />
            </Button>
          </div>
          {runtimes.length === 0 ? (
            <p className="text-sm text-[hsl(var(--secondary))]">{t("deployment.java.jdk.none")}</p>
          ) : (
            <ul className="divide-y rounded-lg border">
              {runtimes.map((runtime) => (
                <li key={runtime.binary} className="flex items-center gap-2 px-3 py-2 text-sm">
                  <div className="min-w-0 flex-1">
                    <div className="font-medium">{runtime.version || "—"}</div>
                    <div className="truncate font-mono text-xs text-[hsl(var(--secondary))]" title={runtime.binary}>{runtime.binary}</div>
                  </div>
                  {runtime.isDefault ? (
                    <span className="shrink-0 text-xs text-emerald-500">{t("deployment.java.jdk.default")}</span>
                  ) : (
                    <Button size="sm" variant="ghost" disabled={Boolean(pending)} onClick={() => makeDefault(runtime)}>
                      {t("deployment.java.jdk.setDefault")}
                    </Button>
                  )}
                </li>
              ))}
            </ul>
          )}
          <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.java.jdk.defaultHint")}</p>
          {output && (
            <pre className={`max-h-48 overflow-auto whitespace-pre-wrap rounded-lg bg-slate-950 p-3 font-mono text-xs ${failed ? "text-red-300" : "text-slate-100"}`}>{output}</pre>
          )}
        </div>
      </DialogShell>
      {pending && <ConfirmOperation pending={pending} onClose={() => setPending(null)} />}
    </>
  );
}
