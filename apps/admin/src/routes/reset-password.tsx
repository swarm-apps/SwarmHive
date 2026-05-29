import { Trans, useLingui } from "@lingui/react/macro";
import { useMutation, useQuery } from "@tanstack/react-query";
import { createFileRoute, Link, useRouter } from "@tanstack/react-router";
import { App, Button, Card, Form, Input, Result, Spin, Typography } from "antd";
import { useState } from "react";
import { z } from "zod";
import { isApiError } from "@/lib/api";
import { ERR_PASSWORD_TOO_WEAK, postResetPassword, resetInfoQueryOptions } from "@/lib/api/account";
import { confirmPasswordRules, passwordRules } from "@/lib/validation/password";

const searchSchema = z.object({
  token: z.string().min(1),
});

interface FormValues {
  password: string;
  confirm: string;
}

export const Route = createFileRoute("/reset-password")({
  validateSearch: searchSchema,
  component: ResetPasswordPage,
});

function ResetPasswordPage() {
  const { t } = useLingui();
  const router = useRouter();
  const { notification } = App.useApp();
  const { token } = Route.useSearch();
  const [form] = Form.useForm<FormValues>();
  const [passwordError, setPasswordError] = useState<string | null>(null);

  const info = useQuery(resetInfoQueryOptions(token));

  const mutation = useMutation({
    mutationFn: ({ password }: FormValues) => postResetPassword(token, password),
    onError: (error) => {
      setPasswordError(null);
      if (isApiError(error) && error.type === ERR_PASSWORD_TOO_WEAK) {
        setPasswordError(error.detail ?? t`密码强度不足。`);
        return;
      }
      notification.error({ message: t`提交失败，请稍后重试。` });
    },
    onSuccess: () => {
      notification.success({ message: t`密码已重置，请用新密码登录。` });
      router.navigate({ to: "/login", replace: true });
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
            title={<Trans>重置链接无效或已过期</Trans>}
            subTitle={<Trans>请回到“忘记密码”页面重新申请；链接只能使用一次。</Trans>}
            extra={
              <Link to="/forgot-password">
                <Button type="primary">
                  <Trans>重新申请</Trans>
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
      <Card style={{ width: 420 }} title={<Trans>重置密码</Trans>}>
        <Typography.Paragraph type="secondary">
          <Trans>
            为账号 <Typography.Text strong>{info.data?.email}</Typography.Text> 设置新密码。
          </Trans>
        </Typography.Paragraph>
        <Form<FormValues>
          form={form}
          layout="vertical"
          requiredMark={false}
          onFinish={(values) => mutation.mutate(values)}
        >
          <Form.Item
            label={<Trans>新密码</Trans>}
            name="password"
            help={passwordError ?? undefined}
            validateStatus={passwordError ? "error" : undefined}
            rules={passwordRules(t)}
          >
            <Input.Password autoComplete="new-password" autoFocus />
          </Form.Item>
          <Form.Item
            label={<Trans>确认密码</Trans>}
            name="confirm"
            dependencies={["password"]}
            rules={confirmPasswordRules(t, () => form.getFieldValue("password"))}
          >
            <Input.Password autoComplete="new-password" />
          </Form.Item>
          <Button type="primary" htmlType="submit" block loading={mutation.isPending}>
            <Trans>提交</Trans>
          </Button>
        </Form>
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
