import { Trans, useLingui } from "@lingui/react/macro";
import { useMutation } from "@tanstack/react-query";
import { createFileRoute, Link, useRouter } from "@tanstack/react-router";
import { App, Button, Card, Form, Input, Result, Space, Typography } from "antd";
import { useState } from "react";
import { isApiError } from "@/lib/api";
import { postForgotPassword } from "@/lib/api/account";

interface FormValues {
  email: string;
}

export const Route = createFileRoute("/forgot-password")({
  component: ForgotPasswordPage,
});

function ForgotPasswordPage() {
  const { t } = useLingui();
  const router = useRouter();
  const { notification } = App.useApp();
  const [form] = Form.useForm<FormValues>();
  // Switches to a success screen unconditionally on 200 — server hides
  // whether the email actually exists, so the UI must too.
  const [submitted, setSubmitted] = useState(false);

  const mutation = useMutation({
    mutationFn: ({ email }: FormValues) => postForgotPassword(email.trim()),
    onSuccess: () => setSubmitted(true),
    onError: (error) => {
      if (isApiError(error) && error.status === 429) {
        notification.warning({
          message: t`请求过于频繁`,
          description: t`请稍后再试。`,
        });
        return;
      }
      notification.error({ message: t`提交失败，请稍后重试。` });
    },
  });

  if (submitted) {
    return (
      <PageShell>
        <Card style={{ width: 480 }}>
          <Result
            status="success"
            title={<Trans>若该邮箱已注册，我们已发送重置链接</Trans>}
            subTitle={
              <Trans>请检查收件箱（含垃圾邮件夹）。链接 60 分钟内有效，仅可使用一次。</Trans>
            }
            extra={
              <Button type="primary" onClick={() => router.navigate({ to: "/login" })}>
                <Trans>返回登录</Trans>
              </Button>
            }
          />
        </Card>
      </PageShell>
    );
  }

  return (
    <PageShell>
      <Card style={{ width: 400 }} title={<Trans>找回密码</Trans>}>
        <Typography.Paragraph type="secondary">
          <Trans>输入注册邮箱，我们会发送一封含重置链接的邮件。</Trans>
        </Typography.Paragraph>
        <Form<FormValues>
          form={form}
          layout="vertical"
          requiredMark={false}
          onFinish={(values) => mutation.mutate(values)}
        >
          <Form.Item
            label={<Trans>邮箱</Trans>}
            name="email"
            rules={[
              { required: true, message: t`请输入邮箱` },
              { type: "email", message: t`邮箱格式不正确` },
            ]}
          >
            <Input placeholder={t`you@example.com`} autoComplete="username" autoFocus />
          </Form.Item>
          <Button type="primary" htmlType="submit" block loading={mutation.isPending}>
            <Trans>发送重置链接</Trans>
          </Button>
        </Form>
        <Space style={{ marginTop: 16 }}>
          <Link to="/login">
            <Trans>返回登录</Trans>
          </Link>
        </Space>
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
