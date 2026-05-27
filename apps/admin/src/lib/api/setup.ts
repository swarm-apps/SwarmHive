import { $api, type paths } from ".";

/** Response shape from `GET /api/v1/setup/info`. */
export type SetupInfo =
  paths["/api/v1/setup/info"]["get"]["responses"][200]["content"]["application/json"];

/** Request body for `POST /api/v1/setup`. */
export type SetupRequest = NonNullable<
  paths["/api/v1/setup"]["post"]["requestBody"]
>["content"]["application/json"];

export const SETUP_INFO_PATH = "/api/v1/setup/info" as const;
export const SETUP_PATH = "/api/v1/setup" as const;

/**
 * Cached at 60s — bootstrap state flips exactly once per deployment lifetime,
 * so the SPA only needs to refresh occasionally to notice when another tab
 * completed setup. The root `beforeLoad` consumes this to route between
 * `/setup` and `/login`.
 */
export function setupInfoQueryOptions() {
  return $api.queryOptions("get", SETUP_INFO_PATH, undefined, {
    staleTime: 60_000,
    retry: false,
  });
}
