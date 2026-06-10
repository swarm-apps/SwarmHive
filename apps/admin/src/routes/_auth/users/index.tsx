import { createFileRoute, redirect } from "@tanstack/react-router";

/**
 * `/users` 只做转发:成员区是「成员列表 + 注册审批」两个子页的父菜单。
 * 子项路径若与父菜单 path 重叠(都叫 /users),ProLayout 以 path 为 menu key
 * 会撞 key 导致选中态失效——与 `/settings` → `/settings/mail` 同款解法。
 */
export const Route = createFileRoute("/_auth/users/")({
  beforeLoad: () => {
    throw redirect({ to: "/users/list", replace: true });
  },
});
