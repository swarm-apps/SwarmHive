import { $api, type components, fetchClient } from ".";

// ────────────────────────── Schema 派生类型 ──────────────────────────

export type WebhookEndpoint = components["schemas"]["WebhookEndpoint"];
export type WebhookProviderKind = components["schemas"]["WebhookProviderKind"];
export type CreateWebhookEndpointReq = components["schemas"]["CreateWebhookEndpointReq"];
export type UpdateWebhookEndpointReq = components["schemas"]["UpdateWebhookEndpointReq"];
export type CreateWebhookEndpointResp = components["schemas"]["CreateWebhookEndpointResp"];
export type RotateSecretResp = components["schemas"]["RotateSecretResp"];
export type WebhookEndpointTestResp = components["schemas"]["WebhookEndpointTestResp"];
export type Subscription = components["schemas"]["Subscription"];
export type CreateSubscriptionReq = components["schemas"]["CreateSubscriptionReq"];
export type Delivery = components["schemas"]["Delivery"];
export type DeliveryDetail = components["schemas"]["DeliveryDetail"];
export type NotificationEventType = components["schemas"]["NotificationEventType"];
export type NotificationChannelKind = components["schemas"]["NotificationChannelKind"];
export type DeliveryStatus = components["schemas"]["DeliveryStatus"];

// ────────────────────────── 路径常量 ──────────────────────────

export const ENDPOINTS_PATH = "/api/v1/notifications/webhook-endpoints" as const;
const ENDPOINT_PATH = "/api/v1/notifications/webhook-endpoints/{id}" as const;
const ROTATE_PATH = "/api/v1/notifications/webhook-endpoints/{id}/rotate-secret" as const;
const TEST_PATH = "/api/v1/notifications/webhook-endpoints/{id}/test" as const;
export const SUBSCRIPTIONS_PATH = "/api/v1/notifications/subscriptions" as const;
const SUBSCRIPTION_PATH = "/api/v1/notifications/subscriptions/{id}" as const;
export const DELIVERIES_PATH = "/api/v1/notifications/deliveries" as const;
const DELIVERY_PATH = "/api/v1/notifications/deliveries/{id}" as const;
const REDELIVER_PATH = "/api/v1/notifications/deliveries/{id}/attempts" as const;

// MVP 三个发布列车事件类型(wire = CloudEvents 风格点分串)。
export const EVENT_TYPES: readonly NotificationEventType[] = [
  "release.published",
  "channel.promoted",
  "channel.rolled_back",
];

// ────────────────────────── List query options ──────────────────────────

export function endpointsQueryOptions() {
  return $api.queryOptions("get", ENDPOINTS_PATH);
}

export function subscriptionsQueryOptions() {
  return $api.queryOptions("get", SUBSCRIPTIONS_PATH);
}

/** 投递详情(含请求/响应快照)。`enabled` 控制懒加载——仅行展开时才拉。 */
export function deliveryDetailQueryOptions(id: string, enabled: boolean) {
  return $api.queryOptions("get", DELIVERY_PATH, { params: { path: { id } } }, { enabled });
}

// ────────────────────────── Imperative helpers ──────────────────────────
// 与 apps.ts / account.ts 一致:middleware 在非 2xx 抛 ApiError,调用方 catch
// 后取 error.detail 展示。

export async function createEndpoint(
  body: CreateWebhookEndpointReq,
): Promise<CreateWebhookEndpointResp> {
  const { data, error } = await fetchClient.POST(ENDPOINTS_PATH, { body });
  if (error) throw error;
  if (!data) throw new Error("create endpoint response missing body");
  return data;
}

export async function updateEndpoint(
  id: string,
  body: UpdateWebhookEndpointReq,
): Promise<WebhookEndpoint> {
  const { data, error } = await fetchClient.PATCH(ENDPOINT_PATH, {
    params: { path: { id } },
    body,
  });
  if (error) throw error;
  if (!data) throw new Error("update endpoint response missing body");
  return data;
}

export async function deleteEndpoint(id: string): Promise<void> {
  const { error } = await fetchClient.DELETE(ENDPOINT_PATH, { params: { path: { id } } });
  if (error) throw error;
}

export async function rotateSecret(id: string): Promise<RotateSecretResp> {
  const { data, error } = await fetchClient.POST(ROTATE_PATH, { params: { path: { id } } });
  if (error) throw error;
  if (!data) throw new Error("rotate secret response missing body");
  return data;
}

export async function testEndpoint(id: string): Promise<WebhookEndpointTestResp> {
  const { data, error } = await fetchClient.POST(TEST_PATH, { params: { path: { id } } });
  if (error) throw error;
  if (!data) throw new Error("test endpoint response missing body");
  return data;
}

export async function createSubscription(body: CreateSubscriptionReq): Promise<Subscription> {
  const { data, error } = await fetchClient.POST(SUBSCRIPTIONS_PATH, { body });
  if (error) throw error;
  if (!data) throw new Error("create subscription response missing body");
  return data;
}

export async function deleteSubscription(id: string): Promise<void> {
  const { error } = await fetchClient.DELETE(SUBSCRIPTION_PATH, { params: { path: { id } } });
  if (error) throw error;
}

export async function redeliver(id: string): Promise<Delivery> {
  const { data, error } = await fetchClient.POST(REDELIVER_PATH, { params: { path: { id } } });
  if (error) throw error;
  if (!data) throw new Error("redeliver response missing body");
  return data;
}

// ────────────────────────── 纯函数(可单测) ──────────────────────────

/**
 * delivery 四态 → AntD Tag 语义色(design.md D3)。
 * failed 与 dead 必须肉眼可分:failed=warning(橙,还会自动重试)、dead=error(红,终态)。
 */
export function deliveryStatusColor(status: DeliveryStatus): string {
  switch (status) {
    case "sent":
      return "success";
    case "pending":
      return "processing";
    case "failed":
      return "warning";
    case "dead":
      return "error";
    default:
      return "default";
  }
}

/** endpoint 行的「订阅数」:客户端 join 订阅列表(零后端)。 */
export function countSubscriptionsForEndpoint(
  subscriptions: Subscription[],
  endpointId: string,
): number {
  return subscriptions.filter((s) => s.webhook_endpoint_id === endpointId).length;
}

/** webhook endpoint id → 名称(订阅 / 投递列表展示用);找不到或空 id 返回 undefined。 */
export function endpointName(
  endpoints: WebhookEndpoint[],
  id: string | null | undefined,
): string | undefined {
  if (!id) return undefined;
  return endpoints.find((e) => e.id === id)?.name;
}
