import { createFileRoute, Outlet } from "@tanstack/react-router";

/**
 * Pass-through layout for the organisation/deployment-level settings tree.
 * Sub-modules (Mail / Auth / Storage / Telemetry) are exposed via the ProLayout
 * sub-menu in `_auth/route.tsx`, visible only to `*:manage` holders; no
 * second-level Sider here. `_auth/settings/index.tsx` redirects `/settings` to
 * the first module the user can manage (or `/profile` if none). Personal
 * account management lives at `/profile`, not under settings.
 */
export const Route = createFileRoute("/_auth/settings")({
  component: () => <Outlet />,
});
