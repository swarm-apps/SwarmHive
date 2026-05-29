import { $api, type components } from ".";

export type MailProvider = components["schemas"]["MailProviderView"];
export type MailTemplate = components["schemas"]["MailTemplateView"];
export type MailLog = components["schemas"]["MailLogView"];
export type CreateProviderReq = components["schemas"]["CreateProviderReq"];
export type UpdateProviderReq = components["schemas"]["UpdateProviderReq"];
export type PreviewResp = components["schemas"]["PreviewResp"];

export const PROVIDERS_PATH = "/api/v1/mail/providers" as const;
export const TEMPLATES_PATH = "/api/v1/mail/templates" as const;
export const LOGS_PATH = "/api/v1/mail/logs" as const;

/**
 * Polled lightly so the SPA fallback banner reacts shortly after the
 * Owner activates a provider. 30 s feels right — short enough to feel
 * live, long enough to ignore tab focus churn.
 */
export function mailStatusQueryOptions() {
  return $api.queryOptions("get", "/api/v1/mail/status", undefined, {
    staleTime: 30_000,
    retry: false,
  });
}

export function mailTemplatesQueryOptions() {
  return $api.queryOptions("get", TEMPLATES_PATH);
}
