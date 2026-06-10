import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, redirect, useRouter } from "@tanstack/react-router";
import { Button, Card, Result } from "antd";
import { useEffect } from "react";
import { isApiError } from "@/lib/api";
import { meQueryOptions } from "@/lib/query/meQuery";

/**
 * 待审批等待页:**顶层路由,不挂 `_auth` 的 ProLayout**——待审批用户不该看到
 * 后台侧边栏壳,这里是全屏居中卡片(与 /register、/verify-email-sent 同款形态)。
 * 认证仍然必需:自己 beforeLoad 拉 me(401 → /login;已 active → /)。
 */
export const Route = createFileRoute("/awaiting-approval")({
  beforeLoad: async ({ context }) => {
    try {
      const me = await context.queryClient.ensureQueryData(meQueryOptions());
      if (me.user.status === "active") {
        throw redirect({ to: "/", replace: true });
      }
    } catch (error) {
      if (isApiError(error) && error.status === 401) {
        throw redirect({ to: "/login", replace: true });
      }
      throw error;
    }
  },
  component: AwaitingApprovalPage,
});

function AwaitingApprovalPage() {
  const { t } = useLingui();
  const router = useRouter();
  const queryClient = useQueryClient();
  // 30s 轮询 me:Owner 批准后 status 翻成 active,自动放行去首页。
  const me = useQuery({ ...meQueryOptions(), refetchInterval: 30_000 });

  useEffect(() => {
    if (me.data?.user.status === "active") {
      router.navigate({ to: "/", replace: true });
    }
  }, [me.data?.user.status, router]);

  return (
    <div
      style={{
        minHeight: "100vh",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: 24,
      }}
    >
      <Card style={{ width: 520 }}>
        <Result
          status="info"
          title={<Trans>账号正在等待管理员审批</Trans>}
          subTitle={
            <Trans>
              你的注册已提交，管理员批准后即可使用全部功能。本页每 30 秒自动检查一次，
              也可以手动刷新。
            </Trans>
          }
          extra={
            <Button
              type="primary"
              loading={me.isFetching}
              onClick={() => queryClient.invalidateQueries({ queryKey: meQueryOptions().queryKey })}
            >
              {t`刷新状态`}
            </Button>
          }
        />
      </Card>
    </div>
  );
}
