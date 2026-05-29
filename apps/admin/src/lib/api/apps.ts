import { $api, type components, fetchClient } from ".";

// ────────────────────────── Schema 派生类型 ──────────────────────────

export type App = components["schemas"]["App"];
export type CreateAppRequest = components["schemas"]["CreateAppRequest"];
export type UpdateAppRequest = components["schemas"]["UpdateAppRequest"];
export type ChannelView = components["schemas"]["ChannelView"];
export type CreateChannelRequest = components["schemas"]["CreateChannelRequest"];
export type UpdateChannelRequest = components["schemas"]["UpdateChannelRequest"];
export type Platform = components["schemas"]["Platform"];

// ────────────────────────── 路径常量 ──────────────────────────

export const APPS_PATH = "/api/v1/apps" as const;
const APP_PATH = "/api/v1/apps/{slug}" as const;
const CHANNELS_PATH = "/api/v1/apps/{slug}/channels" as const;
const CHANNEL_PATH = "/api/v1/apps/{slug}/channels/{name}" as const;

// ────────────────────────── List query options ──────────────────────────

export function appsQueryOptions() {
  return $api.queryOptions("get", APPS_PATH);
}

export function channelsQueryOptions(slug: string) {
  return $api.queryOptions(
    "get",
    CHANNELS_PATH,
    { params: { path: { slug } } },
    { enabled: slug.length > 0 },
  );
}

// ────────────────────────── Imperative helpers ──────────────────────────
// 与 account.ts 一致：middleware 在非 2xx 抛 ApiError，调用方 catch 后按
// error.type 分支。

export async function createApp(body: CreateAppRequest): Promise<App> {
  const { data, error } = await fetchClient.POST(APPS_PATH, { body });
  if (error) throw error;
  if (!data) throw new Error("create app response missing body");
  return data;
}

export async function updateApp(slug: string, body: UpdateAppRequest): Promise<App> {
  const { data, error } = await fetchClient.PATCH(APP_PATH, {
    params: { path: { slug } },
    body,
  });
  if (error) throw error;
  if (!data) throw new Error("update app response missing body");
  return data;
}

export async function deleteApp(slug: string): Promise<void> {
  const { error } = await fetchClient.DELETE(APP_PATH, {
    params: { path: { slug } },
  });
  if (error) throw error;
}

export async function createChannel(
  slug: string,
  body: CreateChannelRequest,
): Promise<ChannelView> {
  const { data, error } = await fetchClient.POST(CHANNELS_PATH, {
    params: { path: { slug } },
    body,
  });
  if (error) throw error;
  if (!data) throw new Error("create channel response missing body");
  return data;
}

// 把某个 channel 设为默认（server 在同一 TX 内取消旧默认）。
export async function setDefaultChannel(slug: string, name: string): Promise<ChannelView> {
  const { data, error } = await fetchClient.PATCH(CHANNEL_PATH, {
    params: { path: { slug, name } },
    body: { is_default: true },
  });
  if (error) throw error;
  if (!data) throw new Error("update channel response missing body");
  return data;
}

// ────────────────────────── 类型化 error 常量 ──────────────────────────
// 集中登记在 errors.ts，这里 re-export 保持 apps 页 import 路径不变。
export { ERR_APP_HAS_RELEASES, ERR_CONFLICT } from "./errors";

// ────────────────────────── Platform 标签映射 ──────────────────────────
// 列表 Tag 与表单多选共用同一份映射，避免散落。文案是 user-visible，
// 但属枚举值的固定展示名，集中在此一处。

export const PLATFORMS: readonly Platform[] = ["tauri-desktop", "react-native-android"];

export function platformLabel(platform: Platform): string {
  switch (platform) {
    case "tauri-desktop":
      return "Tauri Desktop";
    case "react-native-android":
      return "React Native Android";
    default:
      return platform;
  }
}
