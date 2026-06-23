import { describe, expect, it } from "vitest";
import {
  canRotateSecret,
  countSubscriptionsForEndpoint,
  deliveryStatusColor,
  endpointName,
  type Subscription,
  type WebhookEndpoint,
} from "./notifications";

describe("deliveryStatusColor", () => {
  it("maps the four delivery states to distinct semantic colors", () => {
    expect(deliveryStatusColor("sent")).toBe("success");
    expect(deliveryStatusColor("pending")).toBe("processing");
    expect(deliveryStatusColor("failed")).toBe("warning");
    expect(deliveryStatusColor("dead")).toBe("error");
  });

  it("keeps retryable (failed) and terminal (dead) visually distinct", () => {
    // 这是 design.md D3 的核心约束:运维一眼就能区分还会重试 vs 已死信。
    expect(deliveryStatusColor("failed")).not.toBe(deliveryStatusColor("dead"));
  });
});

describe("countSubscriptionsForEndpoint", () => {
  const subscriptions = [
    { id: "1", webhook_endpoint_id: "ep1" },
    { id: "2", webhook_endpoint_id: "ep1" },
    { id: "3", webhook_endpoint_id: "ep2" },
    { id: "4", email_to: "a@b.c" }, // email 通道,不指向任何 endpoint
  ] as Subscription[];

  it("counts only subscriptions pointing at the given endpoint", () => {
    expect(countSubscriptionsForEndpoint(subscriptions, "ep1")).toBe(2);
    expect(countSubscriptionsForEndpoint(subscriptions, "ep2")).toBe(1);
    expect(countSubscriptionsForEndpoint(subscriptions, "ep3")).toBe(0);
  });
});

describe("endpointName", () => {
  const endpoints = [
    { id: "ep1", name: "Slack #releases" },
    { id: "ep2", name: "Ops webhook" },
  ] as WebhookEndpoint[];

  it("resolves an id to its name", () => {
    expect(endpointName(endpoints, "ep1")).toBe("Slack #releases");
  });

  it("returns undefined for null / undefined / unknown id", () => {
    expect(endpointName(endpoints, null)).toBeUndefined();
    expect(endpointName(endpoints, undefined)).toBeUndefined();
    expect(endpointName(endpoints, "nope")).toBeUndefined();
  });
});

describe("canRotateSecret", () => {
  it("allows rotation for generic (and undefined = default generic) endpoints", () => {
    expect(canRotateSecret("generic")).toBe(true);
    expect(canRotateSecret(undefined)).toBe(true);
  });

  it("forbids rotation for IM providers whose secret is user-owned", () => {
    // 后端对非 generic 直接 422——按钮必须隐藏,避免一次必然失败的交互。
    expect(canRotateSecret("feishu")).toBe(false);
    expect(canRotateSecret("slack")).toBe(false);
    expect(canRotateSecret("dingtalk")).toBe(false);
    expect(canRotateSecret("discord")).toBe(false);
  });
});
