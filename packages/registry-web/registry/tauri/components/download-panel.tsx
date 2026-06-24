"use client";

// download-panel —— public website/download page component.
// registry:component。registryDependencies: button, utils。

import {
  type DownloadArtifact,
  type DownloadCatalog,
  getDownloadCatalog,
  selectBestDownload,
} from "@swarm-hive/sdk";
import {
  Download,
  ExternalLink,
  Loader2,
  MonitorDown,
  PackageOpen,
  Smartphone,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export type DownloadLocale = "en" | "zh-CN";

export interface DownloadPanelTexts {
  title: (appName: string) => string;
  description: (version: string, channel: string) => string;
  primaryButton: (filename: string) => string;
  allDownloads: string;
  noDownloads: string;
  loading: string;
  failed: string;
  retry: string;
  checksum: string;
}

const TEXTS: Record<DownloadLocale, DownloadPanelTexts> = {
  en: {
    title: (appName) => `Download ${appName}`,
    description: (version, channel) => `Latest ${channel} release: ${version}`,
    primaryButton: (filename) => `Download ${filename}`,
    allDownloads: "All downloads",
    noDownloads: "No public installers are available for this release.",
    loading: "Loading downloads",
    failed: "Downloads are unavailable.",
    retry: "Retry",
    checksum: "sha256",
  },
  "zh-CN": {
    title: (appName) => `下载 ${appName}`,
    description: (version, channel) => `${channel} 最新版本：${version}`,
    primaryButton: (filename) => `下载 ${filename}`,
    allDownloads: "全部下载",
    noDownloads: "该版本暂无公开安装包。",
    loading: "正在加载下载项",
    failed: "下载项暂不可用。",
    retry: "重试",
    checksum: "sha256",
  },
};

export interface DownloadPanelProps {
  baseUrl: string;
  appSlug: string;
  channel?: string;
  locale?: DownloadLocale;
  texts?: Partial<DownloadPanelTexts>;
  initialCatalog?: DownloadCatalog;
  className?: string;
}

type LoadState = "idle" | "loading" | "ready" | "error";

export function DownloadPanel({
  baseUrl,
  appSlug,
  channel,
  locale = "en",
  texts,
  initialCatalog,
  className,
}: DownloadPanelProps) {
  const t = { ...TEXTS[locale], ...texts };
  const [catalog, setCatalog] = useState<DownloadCatalog | null>(initialCatalog ?? null);
  const [state, setState] = useState<LoadState>(initialCatalog ? "ready" : "idle");
  const [error, setError] = useState<unknown>(null);

  const load = useCallback(
    async (isCancelled?: () => boolean) => {
      setState("loading");
      setError(null);
      try {
        const next = await getDownloadCatalog({ baseUrl, appSlug, channel });
        if (!isCancelled?.()) {
          setCatalog(next);
          setState("ready");
        }
      } catch (cause) {
        if (!isCancelled?.()) {
          setError(cause);
          setState("error");
        }
      }
    },
    [appSlug, baseUrl, channel],
  );

  useEffect(() => {
    if (initialCatalog) return;
    let cancelled = false;
    void load(() => cancelled);
    return () => {
      cancelled = true;
    };
  }, [initialCatalog, load]);

  const primary = useMemo(() => {
    if (!catalog) return null;
    return (
      selectBestDownload(catalog) ?? (catalog.artifacts.length === 1 ? catalog.artifacts[0] : null)
    );
  }, [catalog]);

  if (state === "loading" || state === "idle") {
    return (
      <div className={cn("rounded-lg border bg-background p-5", className)} aria-live="polite">
        <div className="flex items-center gap-3 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" />
          {t.loading}
        </div>
      </div>
    );
  }

  if (state === "error") {
    return (
      <div className={cn("rounded-lg border bg-background p-5", className)} aria-live="polite">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="text-sm text-muted-foreground">{t.failed}</div>
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              setCatalog(null);
              setError(null);
              void load();
            }}
          >
            {t.retry}
          </Button>
        </div>
        {error instanceof Error ? (
          <p className="mt-2 text-xs text-muted-foreground">{error.message}</p>
        ) : null}
      </div>
    );
  }

  if (!catalog) return null;

  return (
    <div className={cn("rounded-lg border bg-background p-5", className)}>
      <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div className="space-y-1">
          <h2 className="text-lg font-semibold tracking-normal">
            {t.title(catalog.app_display_name)}
          </h2>
          <p className="text-sm text-muted-foreground">
            {t.description(catalog.version, catalog.channel)}
          </p>
        </div>
        {primary ? (
          <a
            href={primary.download_url}
            className={cn(
              "inline-flex h-10 shrink-0 items-center justify-center whitespace-nowrap rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90",
            )}
          >
            <Download className="mr-2 size-4" />
            {t.primaryButton(primary.filename)}
          </a>
        ) : null}
      </div>

      {catalog.artifacts.length === 0 ? (
        <div className="mt-5 flex items-center gap-3 rounded-md bg-muted p-3 text-sm text-muted-foreground">
          <PackageOpen className="size-4" />
          {t.noDownloads}
        </div>
      ) : (
        <div className="mt-5 space-y-3">
          <p className="text-sm font-medium">{t.allDownloads}</p>
          <div className="grid gap-2">
            {catalog.artifacts.map((artifact) => (
              <DownloadRow key={artifact.id} artifact={artifact} checksumLabel={t.checksum} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function DownloadRow({
  artifact,
  checksumLabel,
}: {
  artifact: DownloadArtifact;
  checksumLabel: string;
}) {
  return (
    <a
      href={artifact.download_url}
      className="group grid gap-3 rounded-md border p-3 text-sm transition-colors hover:bg-muted/60 sm:grid-cols-[1fr_auto]"
    >
      <span className="min-w-0 space-y-1">
        <span className="flex min-w-0 items-center gap-2">
          {artifact.platform === "react-native-android" ? (
            <Smartphone className="size-4 shrink-0 text-muted-foreground" />
          ) : (
            <MonitorDown className="size-4 shrink-0 text-muted-foreground" />
          )}
          <span className="truncate font-medium">{artifact.filename}</span>
        </span>
        <span className="block truncate text-xs text-muted-foreground">
          {variantLabel(artifact)} · {formatBytes(artifact.size_bytes)} · {checksumLabel}{" "}
          {artifact.sha256.slice(0, 12)}
        </span>
      </span>
      <span className="flex items-center gap-2 text-xs text-muted-foreground group-hover:text-foreground">
        {artifact.kind}
        <ExternalLink className="size-3.5" />
      </span>
    </a>
  );
}

function variantLabel(artifact: DownloadArtifact): string {
  if (artifact.abi) return artifact.abi;
  if (artifact.target) return artifact.target;
  if (artifact.arch) return artifact.arch;
  return artifact.platform;
}

function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"] as const;
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}
