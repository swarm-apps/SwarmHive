// storage —— KeyValueStorage 的 RN 实现,包 @react-native-async-storage/async-storage
// (草稿,需 RN 运行时验证;本仓 vitest 用注入的 mock storage 替代)。
//
// SDK engine 用它持久化 client_id / dismiss-TTL / lastCheckedAt(对齐 tauri-adapter 用
// plugin-store 的角色)。
//
// 依赖:@react-native-async-storage/async-storage。

import AsyncStorage from "@react-native-async-storage/async-storage";
import type { KeyValueStorage } from "@swarm-hive/sdk";

/**
 * 创建 AsyncStorage 支撑的 KeyValueStorage。get 把 AsyncStorage 的 string | null
 * 直接透传(契约一致);set 落盘。
 */
export function createAsyncStorage(): KeyValueStorage {
  return {
    async get(key: string): Promise<string | null> {
      return AsyncStorage.getItem(key);
    },
    async set(key: string, value: string): Promise<void> {
      await AsyncStorage.setItem(key, value);
    },
  };
}
