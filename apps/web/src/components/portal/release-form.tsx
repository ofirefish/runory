"use client";

import { useEffect } from "react";
import { useActionState } from "react";
import { useRouter } from "next/navigation";
import { LoaderCircle, Save } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  createReleaseAction,
  updateReleaseAction,
} from "@/lib/release-actions";
import { initialReleaseActionState, type ReleaseActionState } from "@/lib/release-action-state";
import { getDictionary, type Locale } from "@/lib/i18n";
import type { ReleaseRecord } from "@/lib/releases";

function messageFor(code: ReleaseActionState["code"], t: ReturnType<typeof getDictionary>["admin"]) {
  switch (code) {
    case "updated":
    case "created":
      return t.releasesUpdated;
    case "conflict":
      return t.releasesConflict;
    case "invalid":
      return t.releasesInvalid;
    case "forbidden":
      return t.releasesForbidden;
    case "configuration":
    case "unavailable":
      return t.releasesUnavailable;
    case "missing":
      return t.releasesMissing;
    default:
      return null;
  }
}

function assetDefaults(release?: ReleaseRecord | null) {
  const byPlatform = new Map(release?.assets.map(asset => [asset.platform, asset]) ?? []);
  return {
    windows: byPlatform.get("windows") ?? { format: ".msi", downloadUrl: "" },
    macos_apple: byPlatform.get("macos_apple") ?? { format: ".dmg", downloadUrl: "" },
    macos_intel: byPlatform.get("macos_intel") ?? { format: ".dmg", downloadUrl: "" },
    linux: byPlatform.get("linux") ?? { format: ".AppImage", downloadUrl: "" },
  };
}

export function ReleaseForm({
  locale,
  release,
  readOnly,
}: {
  locale: Locale;
  release?: ReleaseRecord | null;
  readOnly: boolean;
}) {
  const router = useRouter();
  const t = getDictionary(locale);
  const action = release ? updateReleaseAction : createReleaseAction;
  const [state, formAction, pending] = useActionState(action, initialReleaseActionState);
  const defaults = assetDefaults(release);
  const message = messageFor(state.code, t.admin);

  useEffect(() => {
    if (state.code === "created" && state.id) {
      router.push(`/${locale}/admin/releases/${state.id}`);
      router.refresh();
    }
  }, [locale, router, state.code, state.id]);

  return (
    <form action={formAction} className="profile-form">
      <input type="hidden" name="locale" value={locale} />
      {release ? <input type="hidden" name="releaseId" value={release.id} /> : null}
      <label className="field-label">
        {t.admin.releasesVersion}
        <Input name="version" defaultValue={release?.version ?? ""} required maxLength={32} disabled={readOnly} />
      </label>
      <label className="field-label">
        {t.admin.releasesPageUrl}
        <span>{t.admin.releasesPageUrlHint}</span>
        <Input name="releasePageUrl" defaultValue={release?.releasePageUrl ?? ""} disabled={readOnly} placeholder="https://" />
      </label>
      <label className="field-label">
        {t.admin.releasesNotesZh}
        <textarea
          name="notesZh"
          defaultValue={release?.notesZh ?? ""}
          maxLength={8000}
          disabled={readOnly}
          rows={4}
          className="flex w-full rounded-xl border border-border bg-background/60 px-3.5 py-2.5 text-sm text-foreground outline-none transition placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
        />
      </label>
      <label className="field-label">
        {t.admin.releasesNotesEn}
        <textarea
          name="notesEn"
          defaultValue={release?.notesEn ?? ""}
          maxLength={8000}
          disabled={readOnly}
          rows={4}
          className="flex w-full rounded-xl border border-border bg-background/60 px-3.5 py-2.5 text-sm text-foreground outline-none transition placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
        />
      </label>

      <section className="grid gap-4">
        <h2 className="text-base font-semibold">{t.admin.releasesAssets}</h2>
        {(
          [
            ["windows", t.admin.releasesWindows, "windowsUrl", "windowsFormat"],
            ["macos_apple", t.admin.releasesMacosApple, "macosAppleUrl", "macosAppleFormat"],
            ["macos_intel", t.admin.releasesMacosIntel, "macosIntelUrl", "macosIntelFormat"],
            ["linux", t.admin.releasesLinux, "linuxUrl", "linuxFormat"],
          ] as const
        ).map(([platform, label, urlName, formatName]) => (
          <div key={platform} className="grid gap-3 rounded-xl border border-border p-4 sm:grid-cols-[7rem_1fr]">
            <p className="text-sm font-semibold">{label}</p>
            <div className="grid gap-3 sm:grid-cols-[8rem_1fr]">
              <label className="field-label">
                {t.admin.releasesFormat}
                <Input name={formatName} defaultValue={defaults[platform].format} disabled={readOnly} maxLength={32} />
              </label>
              <label className="field-label">
                {t.admin.releasesUrl}
                <Input name={urlName} defaultValue={defaults[platform].downloadUrl} disabled={readOnly} placeholder="https://" />
              </label>
            </div>
          </div>
        ))}
      </section>

      {readOnly ? <p className="form-message" role="status">{t.admin.releasesReadOnly}</p> : null}
      {message ? (
        <p className={`form-message ${state.code === "updated" || state.code === "created" ? "success" : "error"}`} role="status">
          {message}
        </p>
      ) : null}
      {!readOnly ? (
        <Button type="submit" disabled={pending || state.code === "created"}>
          {pending || state.code === "created" ? <LoaderCircle className="animate-spin" size={16} /> : <Save size={16} />}
          {release ? t.admin.releasesSave : t.admin.releasesCreate}
        </Button>
      ) : null}
    </form>
  );
}
