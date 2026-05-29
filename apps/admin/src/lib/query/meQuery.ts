import { $api } from "../api";

export const ME_PATH = "/api/v1/auth/me" as const;

export function meQueryOptions() {
  return $api.queryOptions("get", ME_PATH);
}
