// client_id —— 灰度稳定标识,本地生成一次并持久化(server 灰度分桶的 key)。

import type { KeyValueStorage } from "./ports";

const CLIENT_ID_KEY = "swarmhive.client_id";

/**
 * 读 storage 里的 client_id;无则生成并写回。
 *
 * 默认用 `crypto.randomUUID()`(Tauri webview / Node 原生支持)。RN/Hermes 若无
 * 全局 crypto,由平台 adapter 传入 `generateId`(如 expo-crypto)——保持 core 零平台依赖。
 */
export async function ensureClientId(
  storage: KeyValueStorage,
  generateId: () => string = () => crypto.randomUUID(),
): Promise<string> {
  const existing = await storage.get(CLIENT_ID_KEY);
  if (existing) return existing;
  const id = generateId();
  await storage.set(CLIENT_ID_KEY, id);
  return id;
}
