import { $api, type components, fetchClient } from ".";

// ────────────────────────── Schema 派生类型 ──────────────────────────

export type Release = components["schemas"]["Release"];
export type ReleaseStatus = components["schemas"]["ReleaseStatus"];
export type CreateReleaseRequest = components["schemas"]["CreateReleaseRequest"];
export type UpdateReleaseRequest = components["schemas"]["UpdateReleaseRequest"];
export type Artifact = components["schemas"]["Artifact"];
export type ArtifactKind = components["schemas"]["ArtifactKind"];
export type PromoteRequest = components["schemas"]["PromoteRequest"];
export type RollbackRequest = components["schemas"]["RollbackRequest"];

// ────────────────────────── 路径常量 ──────────────────────────

const RELEASES_PATH = "/api/v1/apps/{slug}/releases" as const;
const RELEASE_PATH = "/api/v1/apps/{slug}/releases/{version}" as const;
const ARTIFACTS_PATH = "/api/v1/apps/{slug}/releases/{version}/artifacts" as const;
const PUBLISH_PATH = "/api/v1/apps/{slug}/releases/{version}/publish" as const;
const YANK_PATH = "/api/v1/apps/{slug}/releases/{version}/yank" as const;
const CHANNEL_RELEASE_PATH = "/api/v1/apps/{slug}/channels/{name}/release" as const;
const PROMOTE_PATH = "/api/v1/apps/{slug}/channels/{name}/promote" as const;
const ROLLBACK_PATH = "/api/v1/apps/{slug}/channels/{name}/rollback" as const;

// ────────────────────────── List query options ──────────────────────────
// 全部以 enabled 守空——app-scoped 页面在 slug 未选时不打无效请求。

export function releasesQueryOptions(slug: string) {
  return $api.queryOptions(
    "get",
    RELEASES_PATH,
    { params: { path: { slug } } },
    { enabled: slug.length > 0 },
  );
}

export function artifactsQueryOptions(slug: string, version: string) {
  return $api.queryOptions(
    "get",
    ARTIFACTS_PATH,
    { params: { path: { slug, version } } },
    { enabled: slug.length > 0 && version.length > 0 },
  );
}

// channel 当前指向的 release（可能为 null——该 channel 还没 promote 过任何版本）。
export function channelReleaseQueryOptions(slug: string, name: string) {
  return $api.queryOptions(
    "get",
    CHANNEL_RELEASE_PATH,
    { params: { path: { slug, name } } },
    { enabled: slug.length > 0 && name.length > 0 },
  );
}

// ────────────────────────── Imperative helpers ──────────────────────────

export async function createRelease(slug: string, body: CreateReleaseRequest): Promise<Release> {
  const { data, error } = await fetchClient.POST(RELEASES_PATH, {
    params: { path: { slug } },
    body,
  });
  if (error) throw error;
  if (!data) throw new Error("create release response missing body");
  return data;
}

export async function updateRelease(
  slug: string,
  version: string,
  body: UpdateReleaseRequest,
): Promise<Release> {
  const { data, error } = await fetchClient.PATCH(RELEASE_PATH, {
    params: { path: { slug, version } },
    body,
  });
  if (error) throw error;
  if (!data) throw new Error("update release response missing body");
  return data;
}

export async function publishRelease(slug: string, version: string): Promise<Release> {
  const { data, error } = await fetchClient.POST(PUBLISH_PATH, {
    params: { path: { slug, version } },
  });
  if (error) throw error;
  if (!data) throw new Error("publish response missing body");
  return data;
}

export async function yankRelease(slug: string, version: string): Promise<Release> {
  const { data, error } = await fetchClient.POST(YANK_PATH, {
    params: { path: { slug, version } },
  });
  if (error) throw error;
  if (!data) throw new Error("yank response missing body");
  return data;
}

export async function promote(slug: string, name: string, body: PromoteRequest): Promise<Release> {
  const { data, error } = await fetchClient.POST(PROMOTE_PATH, {
    params: { path: { slug, name } },
    body,
  });
  if (error) throw error;
  if (!data) throw new Error("promote response missing body");
  return data;
}

export async function rollback(
  slug: string,
  name: string,
  body: RollbackRequest,
): Promise<Release> {
  const { data, error } = await fetchClient.POST(ROLLBACK_PATH, {
    params: { path: { slug, name } },
    body,
  });
  if (error) throw error;
  if (!data) throw new Error("rollback response missing body");
  return data;
}

// ────────────────────────── 纯展示 / 判断 helper（可单测） ──────────────────────────

// 状态对应的 AntD Tag 颜色（label 文案走 Lingui，在组件内渲染，故这里只给 color）。
export function releaseStatusColor(status: ReleaseStatus): string {
  switch (status) {
    case "published":
      return "green";
    case "draft":
      return "default";
    case "yanked":
      return "red";
    default:
      return "default";
  }
}

// 仅 draft 可发布、仅 published 可撤回——按钮层面就挡掉非法状态转换。
export function canPublish(status: ReleaseStatus): boolean {
  return status === "draft";
}

export function canYank(status: ReleaseStatus): boolean {
  return status === "published";
}

// promote 候选 = 该 app 的 published 版本（前端从列表过滤，不另调接口）。
export function publishedVersions(releases: Release[]): Release[] {
  return releases.filter((r) => r.status === "published");
}
