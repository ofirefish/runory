import { ArrowRight, Download } from "lucide-react";
import { OsIcon } from "@/components/landing/os-icon";
import type { DownloadAsset, LatestReleaseDownloads } from "@/lib/github-releases";
import type { MarketingCopy } from "@/lib/marketing-pages";

type DownloadPanelCopy = MarketingCopy["pages"]["download"]["panel"];

function rowTitle(asset: DownloadAsset, copy: DownloadPanelCopy): string {
  return copy.labels[asset.labelKey];
}

function rowMeta(asset: DownloadAsset, copy: DownloadPanelCopy): string {
  if (asset.labelKey === "windows") return `${asset.format} · ${copy.arch.x64}`;
  if (asset.labelKey === "macosApple") return `${asset.format} · ${copy.arch.appleSilicon}`;
  if (asset.labelKey === "macosIntel") return `${asset.format} · ${copy.arch.intel}`;
  return `${asset.format} · ${copy.arch.x64}`;
}

export function DownloadPlatformPanel({
  downloads,
  copy,
}: {
  downloads: LatestReleaseDownloads;
  copy: DownloadPanelCopy;
}) {
  return (
    <div className="download-platform-panel" id="download-platforms" aria-label={copy.title}>
      <div className="download-platform-header">
        <div className="download-platform-icon">
          <Download size={24} strokeWidth={1.6} />
        </div>
        <div>
          <p className="download-platform-eyebrow">{copy.eyebrow}</p>
          <h2>{copy.title}</h2>
          {downloads.version ? <p className="download-platform-version">{copy.version.replace("{{version}}", downloads.version)}</p> : null}
        </div>
      </div>
      <div className="download-platform-list">
        {downloads.assets.map(asset => (
          <a
            key={asset.id}
            className="download-platform-row"
            href={asset.href}
            target="_blank"
            rel="noopener noreferrer"
          >
            <span className={`download-platform-os download-platform-os-${asset.os}`}>
              <OsIcon os={asset.os} width={18} height={18} />
            </span>
            <span className="download-platform-copy">
              <strong>{rowTitle(asset, copy)}</strong>
              <span>{rowMeta(asset, copy)}</span>
            </span>
            <span className="download-platform-action">
              {asset.direct ? copy.download : copy.browse}
              <ArrowRight size={16} />
            </span>
          </a>
        ))}
      </div>
      <a className="download-platform-all" href={downloads.releasePageUrl} target="_blank" rel="noopener noreferrer">
        {copy.allReleases}
        <ArrowRight size={15} />
      </a>
    </div>
  );
}
