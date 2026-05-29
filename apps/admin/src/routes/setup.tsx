import { Trans, useLingui } from "@lingui/react/macro";
import { useMutation, useQuery } from "@tanstack/react-query";
import { createFileRoute, redirect, useRouter } from "@tanstack/react-router";
import { Alert, App, Button, Card, Form, Input, Typography } from "antd";
import { useState } from "react";
import { fetchClient, isApiError } from "@/lib/api";
import { type SetupRequest, setupInfoQueryOptions } from "@/lib/api/setup";

interface FormValues {
  email: string;
  display_name: string;
  password: string;
  confirm: string;
}

export const Route = createFileRoute("/setup")({
  /**
   * Defensive re-check: the root beforeLoad already routes empty/non-empty
   * deployments correctly, but if a second browser tab finishes setup
   * between the root check and this one, redirect to `/login` instead of
   * showing a stale form.
   */
  beforeLoad: async ({ context }) => {
    const info = await context.queryClient.ensureQueryData(setupInfoQueryOptions());
    if (!info.needs_bootstrap) {
      throw redirect({ to: "/login", replace: true });
    }
  },
  component: SetupPage,
});

function SetupPage() {
  const { t } = useLingui();
  const router = useRouter();
  const { notification } = App.useApp();
  const [form] = Form.useForm<FormValues>();
  // Surfaces the typed `password-too-weak` problem detail inline beside the
  // password field rather than as a generic notification.
  const [passwordError, setPasswordError] = useState<string | null>(null);

  // The root beforeLoad has already populated this; useQuery here just
  // reads from cache + keeps the locked_email-pinned field live if the env
  // value changes (unusual but harmless).
  const info = useQuery(setupInfoQueryOptions());
  const lockedEmail = info.data?.locked_email ?? null;

  const mutation = useMutation({
    mutationFn: async (values: FormValues) => {
      const body: SetupRequest = {
        email: values.email.trim(),
        display_name: values.display_name.trim(),
        password: values.password,
      };
      const { error, response } = await fetchClient.POST("/api/v1/setup", { body });
      if (error) {
        // Middleware already converted to ApiError; the error object here
        // is the parsed problem+json fields we attached on the ApiError.
        throw error;
      }
      if (!response.ok) {
        // Unreachable in practice (middleware throws). Surface generically.
        throw new Error(`HTTP ${response.status}`);
      }
    },
    onError: (error) => {
      setPasswordError(null);
      if (!isApiError(error)) {
        notification.error({ message: t`提交失败，请稍后重试。` });
        return;
      }
      switch (error.type) {
        case "https://swarmhive.dev/errors/password-too-weak":
          setPasswordError(error.detail ?? t`密码强度不足。`);
          break;
        case "https://swarmhive.dev/errors/bootstrap-email-mismatch":
          notification.error({
            message: t`邮箱不匹配`,
            description: error.detail,
          });
          break;
        case "https://swarmhive.dev/errors/bootstrap-already-complete":
          notification.warning({
            message: t`初始化已完成`,
            description: t`正在跳转登录页。`,
          });
          router.navigate({ to: "/login", replace: true });
          break;
        default:
          notification.error({
            message: error.title ?? t`提交失败`,
            description: error.detail,
          });
      }
    },
    onSuccess: () => {
      // Owner row exists now → bootstrap state flips. Invalidate so the
      // root beforeLoad re-evaluates on the next navigation.
      info.refetch();
      router.navigate({ to: "/", replace: true });
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
      <Card style={{ width: 480 }} title={<Trans>初始化 SwarmHive</Trans>}>
        <Typography.Paragraph type="secondary" style={{ marginBottom: 16 }}>
          <Trans>创建首位 Owner 账号。完成后此页面会自动关闭，后续访问统一走 /login。</Trans>
        </Typography.Paragraph>
        {lockedEmail ? (
          <Alert
            type="info"
            showIcon
            style={{ marginBottom: 16 }}
            message={<Trans>已通过环境变量 SWARMHIVE_BOOTSTRAP_OWNER_EMAIL 锁定 Owner 邮箱</Trans>}
            description={lockedEmail}
          />
        ) : null}
        <Form<FormValues>
          form={form}
          layout="vertical"
          requiredMark={false}
          initialValues={lockedEmail ? { email: lockedEmail } : undefined}
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
            <Input
              placeholder={t`you@example.com`}
              disabled={lockedEmail !== null}
              autoComplete="username"
            />
          </Form.Item>
          <Form.Item
            label={<Trans>显示名称</Trans>}
            name="display_name"
            rules={[
              { required: true, message: t`请输入显示名称` },
              { max: 64, message: t`最长 64 个字符` },
            ]}
          >
            <Input placeholder={t`例如：管理员`} />
          </Form.Item>
          <Form.Item
            label={<Trans>密码</Trans>}
            name="password"
            help={passwordError ?? undefined}
            validateStatus={passwordError ? "error" : undefined}
            rules={[
              { required: true, message: t`请输入密码` },
              { min: 12, message: t`密码至少 12 个字符` },
            ]}
          >
            <Input.Password autoComplete="new-password" />
          </Form.Item>
          <Form.Item
            label={<Trans>确认密码</Trans>}
            name="confirm"
            dependencies={["password"]}
            rules={[
              { required: true, message: t`请再次输入密码` },
              ({ getFieldValue }) => ({
                validator: (_, value) =>
                  !value || value === getFieldValue("password")
                    ? Promise.resolve()
                    : Promise.reject(new Error(t`两次输入的密码不一致`)),
              }),
            ]}
          >
            <Input.Password autoComplete="new-password" />
          </Form.Item>
          <Button type="primary" htmlType="submit" block loading={mutation.isPending}>
            <Trans>创建 Owner 并登录</Trans>
          </Button>
        </Form>
      </Card>
    </div>
  );
}
