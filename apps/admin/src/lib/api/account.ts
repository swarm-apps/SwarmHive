import { $api, type components, fetchClient } from ".";

// ────────────────────────── Schema-derived types ──────────────────────────

export type AcceptInviteInfo = components["schemas"]["AcceptInviteInfoResp"];
export type ResetInfo = components["schemas"]["ResetInfoResp"];
export type VerifyInfo = components["schemas"]["VerifyInfoResp"];
export type InviteReq = components["schemas"]["InviteReq"];
export type InviteResp = components["schemas"]["InviteResp"];
export type UserListItem = components["schemas"]["UserListItem"];
export type Role = components["schemas"]["Role"];

// ────────────────────────── List query options ──────────────────────────

export const USERS_PATH = "/api/v1/users" as const;
export const ROLES_PATH = "/api/v1/roles" as const;

export function usersQueryOptions() {
  return $api.queryOptions("get", USERS_PATH);
}

export function rolesQueryOptions() {
  return $api.queryOptions("get", ROLES_PATH, undefined, { staleTime: 300_000 });
}

// ────────────────────────── Token-info query options ──────────────────────────
// Public endpoints — short staleTime keeps the UI from caching a stale
// "valid token" across reload after the user accepted it elsewhere.

export function acceptInviteInfoQueryOptions(token: string) {
  return $api.queryOptions(
    "get",
    "/api/v1/auth/accept-invite/info",
    { params: { query: { token } } },
    { staleTime: 5_000, retry: false, enabled: token.length > 0 },
  );
}

export function resetInfoQueryOptions(token: string) {
  return $api.queryOptions(
    "get",
    "/api/v1/auth/reset-password/info",
    { params: { query: { token } } },
    { staleTime: 5_000, retry: false, enabled: token.length > 0 },
  );
}

export function verifyInfoQueryOptions(token: string) {
  return $api.queryOptions(
    "get",
    "/api/v1/auth/verify-email/info",
    { params: { query: { token } } },
    { staleTime: 5_000, retry: false, enabled: token.length > 0 },
  );
}

// ────────────────────────── Imperative helpers ──────────────────────────
// `fetchClient.POST` flow is uniform — middleware throws ApiError on non-2xx,
// caller branches on `error.type`.

// 销毁当前 cookie 会话（server 端 `session.delete()`）。无 body、返回 204。
// logout 后调用方负责跳转到 /login 并清理本地缓存。
export async function postLogout(): Promise<void> {
  const { error } = await fetchClient.POST("/api/v1/auth/logout", {});
  if (error) throw error;
}

// self-service 改自己的显示名（`PATCH /api/v1/users/me`），返回更新后的 User。
export async function patchDisplayName(
  displayName: string,
): Promise<components["schemas"]["User"]> {
  const { data, error } = await fetchClient.PATCH("/api/v1/users/me", {
    body: { display_name: displayName },
  });
  if (error) throw error;
  if (!data) throw new Error("update profile response missing body");
  return data;
}

// self-service 改 / 设密码（`PUT /api/v1/users/me/password`），204。
// OAuth-only 用户首次设密时 `currentPassword` 传 undefined（后端忽略）。
// 成功后 server 会踢掉本人其它所有 session（当前 session 已重发）。
export async function changePassword(
  currentPassword: string | undefined,
  newPassword: string,
): Promise<void> {
  const { error } = await fetchClient.PUT("/api/v1/users/me/password", {
    body: { current_password: currentPassword, new_password: newPassword },
  });
  if (error) throw error;
}

export async function postForgotPassword(email: string): Promise<void> {
  const { error } = await fetchClient.POST("/api/v1/auth/forgot-password", {
    body: { email },
  });
  if (error) throw error;
}

export async function postResetPassword(token: string, password: string): Promise<void> {
  const { error } = await fetchClient.POST("/api/v1/auth/reset-password", {
    body: { token, password },
  });
  if (error) throw error;
}

export async function postAcceptInvite(token: string, password: string): Promise<void> {
  const { error } = await fetchClient.POST("/api/v1/auth/accept-invite", {
    body: { token, password },
  });
  if (error) throw error;
}

// 消费 verify token。自助注册者(Provisioned)在此完成状态转移并拿到 session,
// `next` 指示去向(pending_approval / home);banner verify(已 Active)为 null。
export async function postVerifyEmail(
  token: string,
): Promise<components["schemas"]["VerifyConsumeResp"]> {
  const { data, error } = await fetchClient.POST("/api/v1/auth/verify-email", {
    body: { token },
  });
  if (error) throw error;
  if (!data) throw new Error("verify email response missing body");
  return data;
}

export async function postResendVerifyEmail(): Promise<void> {
  const { error } = await fetchClient.POST("/api/v1/users/me/verify-email/send", {});
  if (error) throw error;
}

// 公开按 email 重发(自助注册者无 session,用不了上面的 me/send)。始终 200。
export async function postPublicResendVerifyEmail(email: string): Promise<void> {
  const { error } = await fetchClient.POST("/api/v1/auth/verify-email/resend", {
    body: { email },
  });
  if (error) throw error;
}

// ────────────── 自助注册 + 审批(add-registration-policy-and-self-register) ──────────────

export type RegisterResp = components["schemas"]["RegisterResp"];
export type PublicRegistrationOptions = components["schemas"]["PublicRegistrationOptions"];

/** 公开注册可用性(驱动 /login 注册链接 + /register 提示)。 */
export function registrationOptionsQueryOptions() {
  return $api.queryOptions("get", "/api/v1/auth/registration-options", undefined, {
    staleTime: 60_000,
    retry: false,
  });
}

export async function postRegister(body: {
  email: string;
  display_name: string;
  password: string;
}): Promise<RegisterResp> {
  const { data, error } = await fetchClient.POST("/api/v1/auth/register", { body });
  if (error) throw error;
  if (!data) throw new Error("register response missing body");
  return data;
}

export async function postApproveUser(id: string, roleId?: string): Promise<void> {
  const { error } = await fetchClient.POST("/api/v1/users/{id}/approve", {
    params: { path: { id } },
    body: { role_id: roleId ?? null },
  });
  if (error) throw error;
}

export async function postRejectUser(id: string, reason?: string): Promise<void> {
  const { error } = await fetchClient.POST("/api/v1/users/{id}/reject", {
    params: { path: { id } },
    body: { reason: reason ?? null },
  });
  if (error) throw error;
}

// ────────────── 成员管理:改角色 / 禁用 / 启用 ──────────────

/** 整体替换目标用户的角色绑定(单角色 MVP;不可操作 owner / 自己,server 双重校验)。 */
export async function putUserRole(id: string, roleId: string): Promise<void> {
  const { error } = await fetchClient.PUT("/api/v1/users/{id}/role", {
    params: { path: { id } },
    body: { role_id: roleId },
  });
  if (error) throw error;
}

/** 禁用(同时踢掉其全部会话)。 */
export async function postDisableUser(id: string): Promise<void> {
  const { error } = await fetchClient.POST("/api/v1/users/{id}/disable", {
    params: { path: { id } },
  });
  if (error) throw error;
}

export async function postEnableUser(id: string): Promise<void> {
  const { error } = await fetchClient.POST("/api/v1/users/{id}/enable", {
    params: { path: { id } },
  });
  if (error) throw error;
}

export async function postInvite(body: InviteReq): Promise<InviteResp> {
  const { data, error } = await fetchClient.POST("/api/v1/users/invite", { body });
  if (error) throw error;
  if (!data) throw new Error("invite response missing body");
  return data;
}

export async function postResendInvite(id: string): Promise<InviteResp> {
  const { data, error } = await fetchClient.POST("/api/v1/users/invite/{id}/resend", {
    params: { path: { id } },
  });
  if (error) throw error;
  if (!data) throw new Error("resend response missing body");
  return data;
}

// ────────────────────────── Typed error helpers ──────────────────────────

export const ERR_TOKEN_EXPIRED = "https://swarmhive.dev/errors/token-expired";
export const ERR_TOKEN_NOT_FOUND = "about:blank"; // 404 falls back here
export const ERR_TOKEN_ALREADY_CONSUMED = "https://swarmhive.dev/errors/token-already-consumed";
export const ERR_EMAIL_ALREADY_VERIFIED = "https://swarmhive.dev/errors/email-already-verified";
export const ERR_MAIL_NOT_CONFIGURED = "https://swarmhive.dev/errors/mail-not-configured";
export const ERR_RATE_LIMITED = "https://swarmhive.dev/errors/rate-limited";
export const ERR_CANNOT_INVITE_OWNER = "https://swarmhive.dev/errors/cannot-invite-owner";
export const ERR_EMAIL_ALREADY_TAKEN = "https://swarmhive.dev/errors/email-already-taken";
export const ERR_PASSWORD_TOO_WEAK = "https://swarmhive.dev/errors/password-too-weak";
export const ERR_CURRENT_PASSWORD_INCORRECT =
  "https://swarmhive.dev/errors/current-password-incorrect";
export const ERR_INVALID_DISPLAY_NAME = "https://swarmhive.dev/errors/invalid-display-name";
export const ERR_REGISTRATION_DISABLED = "https://swarmhive.dev/errors/registration-disabled";
export const ERR_EMAIL_DOMAIN_NOT_ALLOWED = "https://swarmhive.dev/errors/email-domain-not-allowed";
