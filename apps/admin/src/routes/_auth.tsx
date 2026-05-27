import { createFileRoute, Outlet, redirect } from "@tanstack/react-router";
import { isApiError } from "@/lib/api";
import { meQueryOptions } from "@/lib/query/meQuery";

export const Route = createFileRoute("/_auth")({
  beforeLoad: async ({ context, location }) => {
    try {
      await context.queryClient.ensureQueryData(meQueryOptions());
    } catch (error) {
      if (isApiError(error) && error.status === 401) {
        throw redirect({
          to: "/login",
          search: { next: location.pathname },
          replace: true,
        });
      }
      throw error;
    }
  },
  component: AuthLayout,
});

function AuthLayout() {
  return <Outlet />;
}
