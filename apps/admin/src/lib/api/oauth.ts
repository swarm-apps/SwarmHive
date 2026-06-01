import { $api, type components, fetchClient } from ".";

// OAuth provider config + sign-in/link helpers (add-oauth-github-and-provider-config).

export type OAuthProviderView = components["schemas"]["OAuthProviderView"];
export type PublicOAuthProvider = components["schemas"]["PublicOAuthProvider"];
export type CreateOAuthProviderReq = components["schemas"]["CreateOAuthProviderReq"];
export type UpdateOAuthProviderReq = components["schemas"]["UpdateOAuthProviderReq"];
export type OAuthTestResult = components["schemas"]["OAuthTestResult"];
export type IdentityLink = components["schemas"]["IdentityLink"];
export type OAuthProviderKind = components["schemas"]["OAuthProviderKind"];

export const PROVIDERS_PATH = "/api/v1/auth/providers" as const;
export const PUBLIC_PROVIDERS_PATH = "/api/v1/auth/oauth/providers" as const;
export const IDENTITY_LINKS_PATH = "/api/v1/auth/me/identity-links" as const;

/** Admin CRUD list (requires auth:manage). */
export function providersQueryOptions() {
  return $api.queryOptions("get", PROVIDERS_PATH);
}

/** Public list driving the /login buttons (no auth). */
export function publicProvidersQueryOptions() {
  return $api.queryOptions("get", PUBLIC_PROVIDERS_PATH, undefined, {
    staleTime: 60_000,
    retry: false,
  });
}

/** The current user's linked identities (Profile page). */
export function identityLinksQueryOptions() {
  return $api.queryOptions("get", IDENTITY_LINKS_PATH);
}

export async function createProvider(body: CreateOAuthProviderReq): Promise<OAuthProviderView> {
  const { data, error } = await fetchClient.POST(PROVIDERS_PATH, { body });
  if (error) throw error;
  if (!data) throw new Error("create provider: missing body");
  return data;
}

export async function updateProvider(
  id: string,
  body: UpdateOAuthProviderReq,
): Promise<OAuthProviderView> {
  const { data, error } = await fetchClient.PUT("/api/v1/auth/providers/{id}", {
    params: { path: { id } },
    body,
  });
  if (error) throw error;
  if (!data) throw new Error("update provider: missing body");
  return data;
}

export async function deleteProvider(id: string): Promise<void> {
  const { error } = await fetchClient.DELETE("/api/v1/auth/providers/{id}", {
    params: { path: { id } },
  });
  if (error) throw error;
}

export async function testProvider(id: string): Promise<OAuthTestResult> {
  const { data, error } = await fetchClient.POST("/api/v1/auth/providers/{id}/test", {
    params: { path: { id } },
  });
  if (error) throw error;
  if (!data) throw new Error("test provider: missing body");
  return data;
}

export async function unlinkIdentity(providerName: string): Promise<void> {
  const { error } = await fetchClient.DELETE("/api/v1/auth/oauth/links/{provider_name}", {
    params: { path: { provider_name: providerName } },
  });
  if (error) throw error;
}

// ── Browser redirect URLs (full-page nav, NOT fetch — the cross-origin hop to
//    the provider only works as a top-level navigation). ──

/** `/login` sign-in button target. */
export function oauthLoginUrl(kind: string, next: string): string {
  return `/api/v1/auth/oauth/${kind}/start?next=${encodeURIComponent(next)}`;
}

/** Profile "Link" button target (GET; the actual link happens post-approval). */
export function oauthLinkStartUrl(kind: string): string {
  return `/api/v1/auth/oauth/providers/link/${kind}/start`;
}
