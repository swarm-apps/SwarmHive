import { $api, type components, fetchClient } from ".";

// ────────────────────────── Schema 派生类型 ──────────────────────────

export type StorageBackendView = components["schemas"]["StorageBackendView"];
export type CreateStorageBackendRequest = components["schemas"]["CreateStorageBackendRequest"];
export type UpdateStorageBackendRequest = components["schemas"]["UpdateStorageBackendRequest"];
export type UrlMode = components["schemas"]["UrlMode"];
export type StorageTestResult = components["schemas"]["StorageTestResult"];

// ────────────────────────── 路径常量 ──────────────────────────

export const BACKENDS_PATH = "/api/v1/storage/backends" as const;
const BACKEND_PATH = "/api/v1/storage/backends/{id}" as const;
const TEST_PATH = "/api/v1/storage/backends/{id}/test" as const;
const ACTIVATE_PATH = "/api/v1/storage/backends/{id}/activate" as const;

// ────────────────────────── List query options ──────────────────────────

export function backendsQueryOptions() {
  return $api.queryOptions("get", BACKENDS_PATH);
}

// ────────────────────────── Imperative helpers ──────────────────────────

export async function createBackend(
  body: CreateStorageBackendRequest,
): Promise<StorageBackendView> {
  const { data, error } = await fetchClient.POST(BACKENDS_PATH, { body });
  if (error) throw error;
  if (!data) throw new Error("create backend response missing body");
  return data;
}

// 编辑：`access_key_secret` 留空时调用方应从 body 中省略该字段（server 语义：缺省 = 保留已存 secret）。
export async function updateBackend(
  id: string,
  body: UpdateStorageBackendRequest,
): Promise<StorageBackendView> {
  const { data, error } = await fetchClient.PATCH(BACKEND_PATH, {
    params: { path: { id } },
    body,
  });
  if (error) throw error;
  if (!data) throw new Error("update backend response missing body");
  return data;
}

export async function testBackend(id: string): Promise<StorageTestResult> {
  const { data, error } = await fetchClient.POST(TEST_PATH, {
    params: { path: { id } },
  });
  if (error) throw error;
  if (!data) throw new Error("test response missing body");
  return data;
}

export async function activateBackend(id: string): Promise<StorageBackendView> {
  const { data, error } = await fetchClient.POST(ACTIVATE_PATH, {
    params: { path: { id } },
  });
  if (error) throw error;
  if (!data) throw new Error("activate response missing body");
  return data;
}

// ────────────────────────── 预设（纯数据，可单测） ──────────────────────────
// 只 prefill 易错的连接语义（force_path_style / url_mode），endpoint/bucket/key 仍由用户填。

export type StoragePresetKey = "rustfs" | "oss" | "custom";

export interface StoragePreset {
  label: string;
  force_path_style: boolean;
  url_mode: UrlMode;
  /** endpoint 输入框的占位提示。 */
  endpointHint: string;
}

export const STORAGE_PRESETS: Record<StoragePresetKey, StoragePreset> = {
  // 自带 RustFS：path-style 寻址、公开桶直链。
  rustfs: {
    label: "RustFS（自带）",
    force_path_style: true,
    url_mode: "public",
    endpointHint: "http://127.0.0.1:9000",
  },
  // 阿里云 OSS：virtual-hosted 寻址，私有桶走预签名。
  oss: {
    label: "阿里云 OSS",
    force_path_style: false,
    url_mode: "signed",
    endpointHint: "https://oss-cn-<region>.aliyuncs.com",
  },
  // 通用 S3 兼容：默认 virtual-hosted + 预签名。
  custom: {
    label: "自定义 S3",
    force_path_style: false,
    url_mode: "signed",
    endpointHint: "",
  },
};
