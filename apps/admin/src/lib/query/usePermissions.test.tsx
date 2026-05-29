import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it } from "vitest";
import { meQueryOptions } from "./meQuery";
import { type PermissionName, usePermissions } from "./usePermissions";

// 用 setQueryData 预置 me 缓存，hook 直接命中缓存读 permissions（不触发真实 fetch）。
function makeWrapper(permissions: PermissionName[]) {
  const client = new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: Number.POSITIVE_INFINITY,
        gcTime: Number.POSITIVE_INFINITY,
        retry: false,
      },
    },
  });
  client.setQueryData(meQueryOptions().queryKey, {
    permissions,
    user: { id: "u1", email: "owner@example.dev" },
  } as never);

  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

describe("usePermissions", () => {
  it("has() 对已授予的权限返回 true、未授予返回 false", () => {
    const wrapper = makeWrapper(["app:create", "app:read"]);
    const { result } = renderHook(() => usePermissions(), { wrapper });

    expect(result.current.has("app:create")).toBe(true);
    expect(result.current.has("app:read")).toBe(true);
    expect(result.current.has("app:delete")).toBe(false);
  });

  it("权限为空时 has() 一律返回 false", () => {
    const wrapper = makeWrapper([]);
    const { result } = renderHook(() => usePermissions(), { wrapper });

    expect(result.current.has("app:create")).toBe(false);
    expect(result.current.has("app:update")).toBe(false);
  });
});
