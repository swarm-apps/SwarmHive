import { createFileRoute, redirect } from "@tanstack/react-router";
import { meQueryOptions } from "@/lib/query/meQuery";

export const Route = createFileRoute("/_auth/settings/")({
  // 「设置」是组织 / 部署级配置：按权限落到首个可管理模块；无任一 manage 权限则
  // 回个人页（普通用户的侧栏本就不显示「设置」，此处兜底直接 URL 访问）。
  beforeLoad: async ({ context }) => {
    const me = await context.queryClient.ensureQueryData(meQueryOptions());
    const perms = new Set(me.permissions);
    if (perms.has("mail:manage")) {
      throw redirect({ to: "/settings/mail", replace: true });
    }
    if (perms.has("auth:manage")) {
      throw redirect({ to: "/settings/authentication", replace: true });
    }
    if (perms.has("storage:manage")) {
      throw redirect({ to: "/settings/storage", replace: true });
    }
    throw redirect({ to: "/profile", replace: true });
  },
});
