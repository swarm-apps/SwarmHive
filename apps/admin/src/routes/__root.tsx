import type { QueryClient } from "@tanstack/react-query";
import { createRootRouteWithContext, Outlet, redirect } from "@tanstack/react-router";
import { lazy, Suspense } from "react";
import { setupInfoQueryOptions } from "@/lib/api/setup";

const TanStackRouterDevtools = import.meta.env.DEV
  ? lazy(() =>
      import("@tanstack/router-devtools").then((m) => ({ default: m.TanStackRouterDevtools })),
    )
  : () => null;

interface RouterContext {
  queryClient: QueryClient;
}

export const Route = createRootRouteWithContext<RouterContext>()({
  /**
   * Bootstrap-aware routing: fetch `/api/v1/setup/info` once per session
   * (60s staleTime) and funnel users to `/setup` when the deployment still
   * needs its first Owner, or away from `/setup` once it doesn't. Runs
   * BEFORE the `_auth` guard so an empty DB never triggers a 401 on `/me`.
   */
  beforeLoad: async ({ context, location }) => {
    const info = await context.queryClient.ensureQueryData(setupInfoQueryOptions());
    if (info.needs_bootstrap) {
      if (location.pathname !== "/setup") {
        throw redirect({ to: "/setup", replace: true });
      }
    } else if (location.pathname === "/setup") {
      throw redirect({ to: "/login", replace: true });
    }
  },
  component: RootShell,
});

/**
 * Minimal root — just renders the matched route. ProLayout / fallback banner
 * / authenticated chrome all live in `_auth/route.tsx` so `/login` and
 * `/setup` stay full-screen.
 */
function RootShell() {
  return (
    <>
      <Outlet />
      {import.meta.env.DEV ? (
        <Suspense fallback={null}>
          <TanStackRouterDevtools />
        </Suspense>
      ) : null}
    </>
  );
}
