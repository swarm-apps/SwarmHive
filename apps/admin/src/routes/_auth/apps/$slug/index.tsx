import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/_auth/apps/$slug/")({
  // App 详情默认落到「版本」tab。
  beforeLoad: ({ params }) => {
    throw redirect({
      to: "/apps/$slug/releases",
      params: { slug: params.slug },
      replace: true,
    });
  },
});
