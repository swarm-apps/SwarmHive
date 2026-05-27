import { createFileRoute, Outlet } from "@tanstack/react-router";

/**
 * Pass-through layout. Settings sub-modules (Mail / Auth / Storage /
 * Telemetry) are exposed via the ProLayout sub-menu in `_auth/route.tsx`;
 * no second-level Sider here. `_auth/settings/index.tsx` redirects to
 * `/settings/mail` so visiting `/settings` lands on the first enabled module.
 */
export const Route = createFileRoute("/_auth/settings")({
  component: () => <Outlet />,
});
