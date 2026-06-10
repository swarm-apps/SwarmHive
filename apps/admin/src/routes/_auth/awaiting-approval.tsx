import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, useRouter } from "@tanstack/react-router";
import { Button, Card, Result } from "antd";
import { useEffect } from "react";
import { meQueryOptions } from "@/lib/query/meQuery";

export const Route = createFileRoute("/_auth/awaiting-approval")({
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
    <div style={{ display: "flex", justifyContent: "center", paddingTop: 64 }}>
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
