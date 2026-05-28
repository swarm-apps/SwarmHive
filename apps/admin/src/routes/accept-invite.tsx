import { Trans, useLingui } from "@lingui/react/macro";
import { useMutation, useQuery } from "@tanstack/react-query";
import { createFileRoute, Link, useRouter } from "@tanstack/react-router";
import { App, Button, Card, Form, Input, Result, Spin, Tag, Typography } from "antd";
import { useState } from "react";
import { z } from "zod";
import { isApiError } from "@/lib/api";
import {
  acceptInviteInfoQueryOptions,
  ERR_PASSWORD_TOO_WEAK,
  postAcceptInvite,
} from "@/lib/api/account";
import { meQueryOptions } from "@/lib/query/meQuery";
import { confirmPasswordRules, passwordRules } from "@/lib/validation/password";

const searchSchema = z.object({
  token: z.string().min(1),
});

interface FormValues {
  password: string;
  confirm: string;
}

export const Route = createFileRoute("/accept-invite")({
  validateSearch: searchSchema,
  component: AcceptInvitePage,
});

function AcceptInvitePage() {
  const { t } = useLingui();
  const router = useRouter();
  const { notification } = App.useApp();
  const { token } = Route.useSearch();
  const [form] = Form.useForm<FormValues>();
  const [passwordError, setPasswordError] = useState<string | null>(null);

  const info = useQuery(acceptInviteInfoQueryOptions(token));

  const mutation = useMutation({
    mutationFn: ({ password }: FormValues) => postAcceptInvite(token, password),
    onError: (error) => {
      setPasswordError(null);
      if (isApiError(error) && error.type === ERR_PASSWORD_TOO_WEAK) {
        setPasswordError(error.detail ?? t`密码强度不足。`);
        return;
      }
      notification.error({ message: t`提交失败，请稍后重试。` });
    },
    onSuccess: async () => {
      // Server has already set the session cookie; primes /me so the
      // landing view doesn't blink through a re-login.
      await router.invalidate();
      router.navigate({ to: "/", replace: true });
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
            title={<Trans>邀请链接无效或已过期</Trans>}
            subTitle={
              <Trans>请联系管理员重新发送邀请。每位用户在任意时刻只能持有一封有效邀请。</Trans>
            }
            extra={
              <Link to="/login">
                <Button type="primary">
                  <Trans>前往登录</Trans>
                </Button>
              </Link>
            }
          />
        </Card>
      </PageShell>
    );
  }

  // Pre-prime me query so the redirect lands authenticated.
  void meQueryOptions();

  return (
    <PageShell>
      <Card style={{ width: 480 }} title={<Trans>欢迎加入 SwarmHive</Trans>}>
        <Typography.Paragraph type="secondary">
          <Trans>
            <Typography.Text strong>{info.data?.inviter_name}</Typography.Text> 邀请你以{" "}
            <Tag color="blue">{info.data?.role_name}</Tag>
            身份加入。请设置密码后激活账号。
          </Trans>
        </Typography.Paragraph>
        <Form<FormValues>
          form={form}
          layout="vertical"
          requiredMark={false}
          initialValues={{}}
          onFinish={(values) => mutation.mutate(values)}
        >
          <Form.Item label={<Trans>邮箱</Trans>}>
            <Input value={info.data?.email} disabled autoComplete="username" />
          </Form.Item>
          <Form.Item label={<Trans>显示名称</Trans>}>
            <Input value={info.data?.display_name} disabled />
          </Form.Item>
          <Form.Item
            label={<Trans>设置密码</Trans>}
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
            <Trans>接受邀请并登录</Trans>
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
