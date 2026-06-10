import { Trans, useLingui } from "@lingui/react/macro";
import { useMutation } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { App, Button, Card, Result } from "antd";
import { z } from "zod";
import { postPublicResendVerifyEmail } from "@/lib/api/account";

const searchSchema = z.object({
  email: z.string().optional(),
});

export const Route = createFileRoute("/verify-email-sent")({
  validateSearch: searchSchema,
  component: VerifyEmailSentPage,
});

function VerifyEmailSentPage() {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const { email } = Route.useSearch();

  const resend = useMutation({
    mutationFn: () => postPublicResendVerifyEmail(email ?? ""),
    // server 端枚举防御:无论账号状态如何都 200——前端也只给统一提示。
    onSettled: () => {
      notification.success({ message: t`如果该邮箱待验证，新的验证邮件已发出。` });
    },
  });

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
      <Card style={{ width: 480 }}>
        <Result
          status="success"
          title={<Trans>验证邮件已发送</Trans>}
          subTitle={
            email ? (
              <Trans>请到 {email} 的收件箱点击验证链接完成注册。</Trans>
            ) : (
              <Trans>请查收注册邮箱中的验证链接完成注册。</Trans>
            )
          }
          extra={[
            email ? (
              <Button key="resend" loading={resend.isPending} onClick={() => resend.mutate()}>
                <Trans>未收到？重新发送</Trans>
              </Button>
            ) : null,
            <Link key="login" to="/login">
              <Button type="primary">
                <Trans>返回登录</Trans>
              </Button>
            </Link>,
          ]}
        />
      </Card>
    </div>
  );
}
