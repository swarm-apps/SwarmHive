import { $api, type components } from ".";

// Telemetry 查询(add-telemetry-events):全部读 rollup 表,day bucket 为 UTC。

export type TelemetrySummary = components["schemas"]["TelemetrySummary"];
export type AdoptionPoint = components["schemas"]["AdoptionPoint"];
export type FunnelStage = components["schemas"]["FunnelStage"];
export type DistributionSlice = components["schemas"]["DistributionSlice"];
export type TelemetryOverview = components["schemas"]["TelemetryOverview"];

/** 首页全局速览(跨所有 app)。`enabled` 让无 telemetry:read 的角色优雅降级。 */
export function telemetryOverviewQueryOptions(days: number, enabled = true) {
  return $api.queryOptions(
    "get",
    "/api/v1/telemetry/overview",
    { params: { query: { days } } },
    { enabled, staleTime: 60_000 },
  );
}

export function telemetrySummaryQueryOptions(app: string, days: number) {
  return $api.queryOptions(
    "get",
    "/api/v1/telemetry/summary",
    { params: { query: { app, days } } },
    { enabled: app.length > 0, staleTime: 60_000 },
  );
}

export function telemetryAdoptionQueryOptions(app: string, days: number) {
  return $api.queryOptions(
    "get",
    "/api/v1/telemetry/adoption",
    { params: { query: { app, days } } },
    { enabled: app.length > 0, staleTime: 60_000 },
  );
}

export function telemetryFunnelQueryOptions(app: string, days: number) {
  return $api.queryOptions(
    "get",
    "/api/v1/telemetry/funnel",
    { params: { query: { app, days } } },
    { enabled: app.length > 0, staleTime: 60_000 },
  );
}

export function telemetryDistributionQueryOptions(app: string, days: number, dim: string) {
  return $api.queryOptions(
    "get",
    "/api/v1/telemetry/distribution",
    { params: { query: { app, days, dim } } },
    { enabled: app.length > 0, staleTime: 60_000 },
  );
}
