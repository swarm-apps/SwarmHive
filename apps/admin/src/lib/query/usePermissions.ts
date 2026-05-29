import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import type { components } from "@/lib/api";
import { meQueryOptions } from "./meQuery";

export type PermissionName = components["schemas"]["PermissionName"];

/**
 * 读取当前登录用户的权限集合，供业务页按 `app:*` / `release:*` 等门控
 * 按钮显隐。复用 `meQueryOptions()`——与 `_auth` guard / avatar 共享同一份
 * react-query 缓存，不产生额外请求。
 *
 * 门控策略：无权限时**不渲染**按钮（而非 disabled），避免点了才 403。
 */
export function usePermissions() {
  const me = useQuery({ ...meQueryOptions(), retry: false });
  const set = useMemo(() => new Set(me.data?.permissions ?? []), [me.data?.permissions]);

  return useMemo(
    () => ({
      has: (permission: PermissionName) => set.has(permission),
      isLoading: me.isPending,
    }),
    [set, me.isPending],
  );
}
