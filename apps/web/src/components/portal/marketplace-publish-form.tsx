"use client";

import { useEffect, useState } from "react";
import { useActionState } from "react";
import { useRouter } from "next/navigation";
import { LoaderCircle, PackagePlus, Upload } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  initialMarketplacePublishState,
  publishMarketplaceAction,
  type MarketplacePublishState,
} from "@/lib/marketplace-actions";
import { getDictionary, type Locale } from "@/lib/i18n";

function messageFor(code: MarketplacePublishState["code"], t: ReturnType<typeof getDictionary>["admin"]) {
  switch (code) {
    case "created":
      return t.storeSuccess;
    case "package_required":
      return t.storeErrorPackage;
    case "too_large_for_serverless":
      return t.storeErrorTooLarge;
    case "version_immutable":
      return t.storeErrorConflict;
    case "invalid":
      return t.storeErrorInvalid;
    case "forbidden":
      return t.storeErrorForbidden;
    case "unavailable":
    case "storage":
    case "db":
    case "error":
      return t.storeErrorUnavailable;
    default:
      return null;
  }
}

export function MarketplacePublishForm({
  locale,
  defaults,
}: {
  locale: Locale;
  defaults?: {
    pluginId?: string;
    publisherId?: string;
    name?: string;
    category?: string;
    description?: string | null;
    verified?: boolean;
  };
}) {
  const router = useRouter();
  const t = getDictionary(locale);
  const [state, formAction, pending] = useActionState(
    publishMarketplaceAction,
    initialMarketplacePublishState,
  );
  const [fileName, setFileName] = useState<string | null>(null);
  const message = messageFor(state.code, t.admin);
  const isError = state.code !== "idle" && state.code !== "created";

  useEffect(() => {
    if (state.code === "created" && state.pluginId) {
      router.push(`/${locale}/admin/store/${encodeURIComponent(state.pluginId)}`);
      router.refresh();
    }
  }, [locale, router, state.code, state.pluginId]);

  return (
    <form action={formAction} className="profile-form">
      <input type="hidden" name="locale" value={locale} />

      <div className="grid gap-4 sm:grid-cols-2">
        <label className="field-label">
          {t.admin.storePluginId}
          <Input
            name="pluginId"
            defaultValue={defaults?.pluginId ?? ""}
            required
            placeholder="runory.hello"
            disabled={Boolean(defaults?.pluginId)}
          />
          <span>{t.admin.storePluginIdHint}</span>
        </label>
        <label className="field-label">
          {t.admin.storeVersion}
          <Input name="version" required placeholder="1.0.0" />
          <span>{t.admin.storeVersionHint}</span>
        </label>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <label className="field-label">
          {t.admin.storeName}
          <Input name="name" defaultValue={defaults?.name ?? ""} required />
        </label>
        <label className="field-label">
          {t.admin.storePublisherId}
          <Input name="publisherId" defaultValue={defaults?.publisherId ?? "runory"} required />
        </label>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <label className="field-label">
          {t.admin.storeCategory}
          <Input name="category" defaultValue={defaults?.category ?? "Development"} />
        </label>
        <label className="field-label">
          {t.admin.storeMinRunory}
          <Input name="minRunoryVersion" defaultValue="0.1.0" />
        </label>
      </div>

      <label className="field-label">
        {t.admin.storeSummary}
        <textarea
          name="description"
          defaultValue={defaults?.description ?? ""}
          rows={3}
          className="min-h-[5rem] w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
        />
      </label>

      <label className="field-label">
        {t.admin.storeChangelog}
        <textarea
          name="changelog"
          rows={3}
          className="min-h-[5rem] w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
          placeholder={t.admin.storeChangelogHint}
        />
      </label>

      <label className="field-label">
        {t.admin.storePackage}
        <div className="flex flex-wrap items-center gap-3 rounded-xl border border-dashed border-border bg-muted/30 px-4 py-5">
          <Upload size={18} className="text-muted-foreground" aria-hidden="true" />
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium">{fileName ?? t.admin.storePackageChoose}</p>
            <p className="text-xs text-muted-foreground">{t.admin.storePackageHint}</p>
          </div>
          <Input
            name="package"
            type="file"
            accept=".runory-plugin,application/octet-stream"
            required
            className="max-w-[14rem] cursor-pointer"
            onChange={(event) => setFileName(event.target.files?.[0]?.name ?? null)}
          />
        </div>
      </label>

      <label className="flex items-start gap-3 rounded-xl border border-border px-4 py-3 text-sm">
        <input
          type="checkbox"
          name="verified"
          defaultChecked={defaults?.verified ?? true}
          className="mt-1"
        />
        <span>
          <span className="font-medium">{t.admin.storeVerified}</span>
          <span className="mt-1 block text-muted-foreground">{t.admin.storeVerifiedHint}</span>
        </span>
      </label>

      <div className="flex flex-wrap items-center gap-3">
        <Button type="submit" disabled={pending}>
          {pending ? <LoaderCircle size={16} className="animate-spin" /> : <PackagePlus size={16} />}
          {pending ? t.admin.storePublishing : t.admin.storeSubmit}
        </Button>
        {message ? (
          <p className={`text-sm ${isError ? "text-destructive" : "text-muted-foreground"}`}>{message}</p>
        ) : null}
      </div>
    </form>
  );
}
