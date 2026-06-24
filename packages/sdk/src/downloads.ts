// Public downloads —— headless helpers for website/download-page integrations.
// This is intentionally separate from the in-app update engine.

import type { components } from "./generated/schema";

export type DownloadCatalog = components["schemas"]["DownloadCatalog"];
export type DownloadArtifact = components["schemas"]["DownloadArtifact"];
export type ArtifactKind = components["schemas"]["ArtifactKind"];

export interface GetDownloadCatalogOptions {
  /** SwarmHive server base URL, e.g. https://hive.example.com. */
  baseUrl: string;
  /** App slug. */
  appSlug: string;
  /** Optional channel. Defaults to the app's default channel. */
  channel?: string;
  /** Inject fetch for tests or non-browser runtimes. */
  fetchImpl?: typeof fetch;
}

export class DownloadCatalogError extends Error {
  readonly phase = "download-catalog" as const;
  readonly cause?: unknown;

  constructor(message: string, cause?: unknown) {
    super(message);
    this.name = "DownloadCatalogError";
    this.cause = cause;
  }
}

export async function getDownloadCatalog(
  opts: GetDownloadCatalogOptions,
): Promise<DownloadCatalog> {
  const doFetch = opts.fetchImpl ?? fetch;
  const base = opts.baseUrl.replace(/\/+$/, "");
  const q = new URLSearchParams();
  if (opts.channel) q.set("channel", opts.channel);
  const query = q.toString();
  const url = `${base}/api/v1/downloads/${encodeURIComponent(opts.appSlug)}${query ? `?${query}` : ""}`;

  let res: Response;
  try {
    res = await doFetch(url, { method: "GET" });
  } catch (cause) {
    throw new DownloadCatalogError("download catalog request failed", cause);
  }
  if (!res.ok) {
    throw new DownloadCatalogError(`download catalog returned HTTP ${res.status}`);
  }
  return (await res.json()) as DownloadCatalog;
}

export type DownloadOs = "macos" | "windows" | "linux" | "android" | "ios" | "unknown";
export type DownloadArch = "x64" | "arm64" | "arm" | "x86" | "universal" | "unknown";

export interface DetectedDownloadPlatform {
  os: DownloadOs;
  arch: DownloadArch;
}

export interface DetectDownloadPlatformOptions {
  userAgent?: string;
  platform?: string;
  maxTouchPoints?: number;
}

export function detectDownloadPlatform(
  opts: DetectDownloadPlatformOptions = {},
): DetectedDownloadPlatform {
  const nav = typeof navigator === "undefined" ? undefined : navigator;
  const userAgent = (opts.userAgent ?? nav?.userAgent ?? "").toLowerCase();
  const platform = (opts.platform ?? nav?.platform ?? "").toLowerCase();
  const maxTouchPoints = opts.maxTouchPoints ?? nav?.maxTouchPoints ?? 0;

  let os: DownloadOs = "unknown";
  if (userAgent.includes("android")) os = "android";
  else if (userAgent.includes("iphone") || userAgent.includes("ipad")) os = "ios";
  else if (platform.includes("mac") || userAgent.includes("mac os x")) {
    os = maxTouchPoints > 1 && userAgent.includes("mobile") ? "ios" : "macos";
  } else if (platform.includes("win") || userAgent.includes("windows")) os = "windows";
  else if (platform.includes("linux") || userAgent.includes("linux")) os = "linux";

  let arch: DownloadArch = "unknown";
  if (userAgent.includes("arm64") || userAgent.includes("aarch64")) arch = "arm64";
  else if (userAgent.includes("armv7") || userAgent.includes("armeabi")) arch = "arm";
  else if (
    userAgent.includes("x86_64") ||
    userAgent.includes("win64") ||
    userAgent.includes("x64") ||
    platform.includes("x86_64") ||
    platform.includes("win64")
  ) {
    arch = "x64";
  } else if (userAgent.includes("i686") || userAgent.includes("x86")) arch = "x86";

  return { os, arch };
}

export function selectBestDownload(
  catalog: DownloadCatalog,
  platform: DetectedDownloadPlatform = detectDownloadPlatform(),
): DownloadArtifact | null {
  let best: { artifact: DownloadArtifact; score: number } | null = null;

  for (const artifact of catalog.artifacts) {
    const score = scoreArtifact(artifact, platform);
    if (score < 0) continue;
    if (!best || score > best.score) best = { artifact, score };
  }

  return best?.artifact ?? null;
}

function scoreArtifact(artifact: DownloadArtifact, platform: DetectedDownloadPlatform): number {
  if (platform.os === "unknown" || platform.os === "ios") return -1;
  if (platform.os === "android" && artifact.platform !== "react-native-android") return -1;
  if (platform.os !== "android" && artifact.platform !== "tauri-desktop") return -1;

  const os = artifactOs(artifact);
  if (os !== "unknown" && os !== platform.os) return -1;

  let score = 10 + kindScore(artifact.kind) + extensionScore(artifact.filename, platform.os);
  const arch = artifactArch(artifact);
  if (platform.arch !== "unknown") {
    if (arch === platform.arch) score += 20;
    else if (arch === "universal") score += 14;
    else if (arch === "unknown") score += 8;
    else score -= 8;
  }
  if (artifact.target || artifact.abi) score += 2;
  return score;
}

function kindScore(kind: ArtifactKind): number {
  switch (kind) {
    case "universal":
      return 6;
    case "installer":
      return 4;
    case "updater":
      return -20;
    default:
      return 0;
  }
}

function extensionScore(filename: string, os: DownloadOs): number {
  const name = filename.toLowerCase();
  if (os === "android") return name.endsWith(".apk") ? 40 : 0;
  if (os === "macos") return name.endsWith(".dmg") ? 40 : 0;
  if (os === "windows") {
    if (name.endsWith(".exe")) return 40;
    if (name.endsWith(".msi")) return 35;
    return 0;
  }
  if (os === "linux") {
    if (name.endsWith(".appimage")) return 40;
    if (name.endsWith(".deb")) return 35;
    if (name.endsWith(".rpm")) return 30;
  }
  return 0;
}

function artifactOs(artifact: DownloadArtifact): DownloadOs {
  if (artifact.platform === "react-native-android") return "android";
  const haystack = `${artifact.target ?? ""} ${artifact.filename}`.toLowerCase();
  if (
    haystack.includes("apple-darwin") ||
    haystack.includes("darwin") ||
    haystack.endsWith(".dmg")
  ) {
    return "macos";
  }
  if (
    haystack.includes("windows") ||
    haystack.includes("pc-windows") ||
    /\.(exe|msi)$/.test(haystack)
  ) {
    return "windows";
  }
  if (
    haystack.includes("linux") ||
    haystack.endsWith(".appimage") ||
    haystack.endsWith(".deb") ||
    haystack.endsWith(".rpm")
  ) {
    return "linux";
  }
  return "unknown";
}

function artifactArch(artifact: DownloadArtifact): DownloadArch {
  const haystack =
    `${artifact.arch ?? ""} ${artifact.target ?? ""} ${artifact.abi ?? ""} ${artifact.filename}`.toLowerCase();
  if (haystack.includes("universal")) return "universal";
  if (haystack.includes("aarch64") || haystack.includes("arm64")) return "arm64";
  if (haystack.includes("armv7") || haystack.includes("armeabi")) return "arm";
  if (haystack.includes("x86_64") || haystack.includes("amd64") || haystack.includes("x64")) {
    return "x64";
  }
  if (haystack.includes("i686") || haystack.includes("x86")) return "x86";
  return "unknown";
}
