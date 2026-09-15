"use client";

import { useActionState } from "react";
import { LoaderCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  initialReleaseActionState,
  setLatestReleaseAction,
  setReleaseStatusAction,
  type ReleaseActionState,
} from "@/lib/release-actions";
import { getDictionary, type Locale } from "@/lib/i18n";
import type { ReleaseStatus } from "@/lib/releases";

function messageFor(code: ReleaseActionState["code"], t: ReturnType<typeof getDictionary>["admin"]) {
  switch (code) {
    case "updated":
      return t.releasesUpdated;
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

export function ReleaseStatusActions({
  locale,
  releaseId,
  status,
  isLatest,
  readOnly,
}: {
  locale: Locale;
  releaseId: string;
  status: ReleaseStatus;
  isLatest: boolean;
  readOnly: boolean;
}) {
  const t = getDictionary(locale);
  const [statusState, statusAction, statusPending] = useActionState(setReleaseStatusAction, initialReleaseActionState);
  const [latestState, latestAction, latestPending] = useActionState(setLatestReleaseAction, initialReleaseActionState);
  const message = messageFor(statusState.code !== "idle" ? statusState.code : latestState.code, t.admin);

  if (readOnly) return <p className="form-message" role="status">{t.admin.releasesReadOnly}</p>;

  return (
    <div className="flex flex-wrap gap-2">
      {status !== "published" ? (
        <form action={statusAction}>
          <input type="hidden" name="locale" value={locale} />
          <input type="hidden" name="releaseId" value={releaseId} />
          <input type="hidden" name="status" value="published" />
          <Button type="submit" disabled={statusPending}>
            {statusPending ? <LoaderCircle className="animate-spin" size={16} /> : null}
            {t.admin.releasesPublish}
          </Button>
        </form>
      ) : (
        <form action={statusAction}>
          <input type="hidden" name="locale" value={locale} />
          <input type="hidden" name="releaseId" value={releaseId} />
          <input type="hidden" name="status" value="draft" />
          <Button type="submit" variant="secondary" disabled={statusPending}>
            {t.admin.releasesUnpublish}
          </Button>
        </form>
      )}
      {status !== "archived" ? (
        <form action={statusAction}>
          <input type="hidden" name="locale" value={locale} />
          <input type="hidden" name="releaseId" value={releaseId} />
          <input type="hidden" name="status" value="archived" />
          <Button type="submit" variant="secondary" disabled={statusPending}>
            {t.admin.releasesArchive}
          </Button>
        </form>
      ) : null}
      {status === "published" && !isLatest ? (
        <form action={latestAction}>
          <input type="hidden" name="locale" value={locale} />
          <input type="hidden" name="releaseId" value={releaseId} />
          <Button type="submit" disabled={latestPending}>
            {latestPending ? <LoaderCircle className="animate-spin" size={16} /> : null}
            {t.admin.releasesSetLatest}
          </Button>
        </form>
      ) : null}
      {message ? <p className={`form-message ${statusState.code === "updated" || latestState.code === "updated" ? "success" : "error"} w-full`} role="status">{message}</p> : null}
    </div>
  );
}
