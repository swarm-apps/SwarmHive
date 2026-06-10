import { Trans, useLingui } from "@lingui/react/macro";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link, useRouter } from "@tanstack/react-router";
import { App, Button, Card, Result, Spin, Typography } from "antd";
import { z } from "zod";
import { postVerifyEmail, verifyInfoQueryOptions } from "@/lib/api/account";
import { meQueryOptions } from "@/lib/query/meQuery";

const searchSchema = z.object({
  token: z.string().min(1),
});

export const Route = createFileRoute("/verify-email")({
  validateSearch: searchSchema,
  component: VerifyEmailPage,
});

function VerifyEmailPage() {
  const router = useRouter();
  const { t } = useLingui();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();
  const { token } = Route.useSearch();

  const info = useQuery(verifyInfoQueryOptions(token));

  const mutation = useMutation({
    mutationFn: () => postVerifyEmail(token),
    onSuccess: async (resp) => {
      notification.success({ message: t`邮箱已验证。` });
      // me.email_verified_at flipped to non-null; banner reads from /me so
      // an invalidate is enough to make it disappear.
      await queryClient.invalidateQueries({ queryKey: meQueryOptions().queryKey });
      // 自助注册者 verify 后已拿到 session,按 server 指示去等待页或首页;
      // banner verify(next 为 null)维持原行为回控制台。
      router.navigate({
        to: resp.next === "pending_approval" ? "/awaiting-approval" : "/",
        replace: true,
      });
    },
    onError: () => {
      notification.error({ message: t`验证失败，请稍后重试或重新申请链接。` });
    },
  });

  if (info.isPending) {
    return (
      <PageShell>
        <Spin />
      </PageShell>
    );
  }

  if (info.isError) {
    return (
      <PageShell>
        <Card style={{ width: 480 }}>
          <Result
            status="error"
            title={<Trans>验证链接无效或已过期</Trans>}
            subTitle={<Trans>请回到控制台顶部 banner，点 “重发验证邮件” 获取新链接。</Trans>}
            extra={
              <Link to="/">
                <Button type="primary">
                  <Trans>返回控制台</Trans>
                </Button>
              </Link>
            }
          />
        </Card>
      </PageShell>
    );
  }

  return (
    <PageShell>
      <Card style={{ width: 480 }} title={<Trans>验证邮箱</Trans>}>
        <Typography.Paragraph>
          <Trans>
            请确认下方邮箱地址是你的：
            <br />
            <Typography.Text strong>{info.data?.email}</Typography.Text>
          </Trans>
        </Typography.Paragraph>
        <Button type="primary" block loading={mutation.isPending} onClick={() => mutation.mutate()}>
          <Trans>确认验证</Trans>
        </Button>
      </Card>
    </PageShell>
  );
}

function PageShell({ children }: { children: React.ReactNode }) {
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
      {children}
    </div>
  );
}
