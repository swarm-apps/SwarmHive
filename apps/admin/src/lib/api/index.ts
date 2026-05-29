export { $api, fetchClient } from "./client";
export type { ProblemBody } from "./error";
export { ApiError, isApiError, parseProblemJson } from "./error";
export type { components, paths } from "./schema.gen";

import type { paths } from "./schema.gen";

export type MeResponse =
  paths["/api/v1/auth/me"]["get"]["responses"][200]["content"]["application/json"];
