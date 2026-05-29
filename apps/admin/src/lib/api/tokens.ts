import dayjs from "dayjs";
import { $api, type components, fetchClient } from ".";

// ────────────────────────── Schema 派生类型 ──────────────────────────

// PermissionName 也在 usePermissions 导出(同源 schema 类型,结构等价);这里在 api
// 层再派生一份,避免 api → query 反向依赖。
export type PermissionName = components["schemas"]["PermissionName"];
export type ApiToken = components["schemas"]["ApiToken"];
export type ApiTokenKind = components["schemas"]["ApiTokenKind"];
export type CreateTokenRequest = components["schemas"]["CreateTokenRequest"];
export type CreateTokenResponse = components["schemas"]["CreateTokenResponse"];

// ────────────────────────── 路径常量 ──────────────────────────

const TOKENS_PATH = "/api/v1/tokens" as const;
const TOKEN_PATH = "/api/v1/tokens/{id}" as const;

// ────────────────────────── List query options ──────────────────────────
// 不带 owner → server 默认列调用者自己的 token(列他人才需 token:manage)。

export function tokensQueryOptions() {
  return $api.queryOptions("get", TOKENS_PATH);
}

// ────────────────────────── Imperative helpers ──────────────────────────

export async function createToken(body: CreateTokenRequest): Promise<CreateTokenResponse> {
  const { data, error } = await fetchClient.POST(TOKENS_PATH, { body });
  if (error) throw error;
  if (!data) throw new Error("create token response missing body");
  return data;
}

export async function revokeToken(id: string): Promise<void> {
  const { error } = await fetchClient.DELETE(TOKEN_PATH, {
    params: { path: { id } },
  });
  if (error) throw error;
}

// ────────────────────────── 纯展示 / 判断 helper（可单测） ──────────────────────────

export type TokenStatus = "active" | "revoked" | "expired";

// 撤销优先于过期:已撤销就是 revoked,否则看是否过期,再否则 active。
export function tokenStatus(t: ApiToken): TokenStatus {
  if (t.revoked_at != null) return "revoked";
  if (t.expires_at != null && dayjs(t.expires_at).isBefore(dayjs())) return "expired";
  return "active";
}

// 状态对应的 AntD Tag 颜色（label 文案走 Lingui，在组件内渲染，故这里只给 color）。
export function tokenStatusColor(status: TokenStatus): string {
  switch (status) {
    case "active":
      return "green";
    case "expired":
      return "default";
    case "revoked":
      return "red";
    default:
      return "default";
  }
}

// PermissionName → 友好名（user-visible，集中一处）。缺省回落原始 wire 串。
const PERMISSION_LABELS: Partial<Record<PermissionName, string>> = {
  "system:manage": "Manage system",
  "user:manage": "Manage users",
  "role:manage": "Manage roles",
  "token:manage": "Manage tokens",
  "storage:manage": "Manage storage",
  "mail:manage": "Manage mail",
  "app:create": "Create apps",
  "app:read": "Read apps",
  "app:update": "Update apps",
  "app:delete": "Delete apps",
  "release:create": "Create releases",
  "release:read": "Read releases",
  "release:update": "Update releases",
  "release:publish": "Publish releases",
  "release:promote": "Promote channels",
  "release:rollback": "Roll back channels",
  "release:yank": "Yank releases",
  "artifact:upload": "Upload artifacts",
  "artifact:read": "Read artifacts",
  "artifact:delete": "Delete artifacts",
  "analytics:read": "Read analytics",
  "telemetry:read": "Read telemetry",
};

export function permissionLabel(p: PermissionName): string {
  return PERMISSION_LABELS[p] ?? p;
}

// 全部内建权限(顺序同 server role.rs)。创建 API token 时按 `has(p)` 过滤出
// 「可授予的子集」= 调用者自己拥有的权限。
export const ALL_PERMISSIONS = Object.keys(PERMISSION_LABELS) as PermissionName[];
