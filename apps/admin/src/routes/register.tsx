import { Trans, useLingui } from "@lingui/react/macro";
import { useMutation, useQuery } from "@tanstack/react-query";
import { createFileRoute, Link, redirect, useRouter } from "@tanstack/react-router";
import { Alert, App, Button, Card, Form, Input } from "antd";
import { isApiError } from "@/lib/api";
import { postRegister, registrationOptionsQueryOptions } from "@/lib/api/account";
import { setupInfoQueryOptions } from "@/lib/api/setup";
import { confirmPasswordRules, passwordRules } from "@/lib/validation/password";

interface FormValues {
  email: string;
  display_name: string;
  password: string;
  confirm: string;
}

export const Route = createFileRoute("/register")({
  // bootstrap 未完成 → /setup(首 Owner 是唯一入口);注册关闭 → 弹回 /login。
  beforeLoad: async ({ context }) => {
    const info = await context.queryClient.ensureQueryData(setupInfoQueryOptions());
    if (info.needs_bootstrap) {
      throw redirect({ to: "/setup", replace: true });
    }
    const options = await context.queryClient.ensureQueryData(registrationOptionsQueryOptions());
    if (!options.allow_self_register_email) {
      throw redirect({ to: "/login", search: { registration_closed: "1" }, replace: true });
    }
  },
  component: RegisterPage,
});

function RegisterPage() {
  const { t } = useLingui();
  const router = useRouter();
  const { notification } = App.useApp();
  const [form] = Form.useForm<FormValues>();
  const options = useQuery(registrationOptionsQueryOptions()).data;

  const mutation = useMutation({
    mutationFn: (values: FormValues) =>
      postRegister({
        email: values.email.trim(),
        display_name: values.display_name.trim(),
        password: values.password,
      }),
    onSuccess: (resp, values) => {
      switch (resp.next) {
        case "verify_email":
          router.navigate({
            to: "/verify-email-sent",
            search: { email: values.email.trim() },
            replace: true,
          });
          break;
        case "pending_approval":
          router.navigate({ to: "/awaiting-approval", replace: true });
          break;
        default:
          router.navigate({ to: "/", replace: true });
      }
    },
    onError: (error) => {
      notification.error({
        message: t`注册失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
    },
  });

  // 按 policy 渲染注册后的去向提示,用户提交前就有预期。
  const hint = options?.require_email_verify ? (
    <Trans>注册后请查收验证邮件完成邮箱验证。</Trans>
  ) : options?.require_approval ? (
    <Trans>注册后需等待管理员审批通过才能使用。</Trans>
  ) : (
    <Trans>注册后即可直接使用。</Trans>
  );

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
      <Card style={{ width: 400 }} title={<Trans>注册 SwarmHive 账号</Trans>}>
        <Alert type="info" showIcon style={{ marginBottom: 16 }} title={hint} />
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
          <Form.Item
            label={<Trans>显示名称</Trans>}
            name="display_name"
            rules={[{ required: true, message: t`请输入显示名称` }]}
          >
            <Input autoComplete="nickname" />
          </Form.Item>
          <Form.Item label={<Trans>密码</Trans>} name="password" rules={passwordRules(t)}>
            <Input.Password autoComplete="new-password" />
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
            <Trans>注册</Trans>
          </Button>
        </Form>
        <div style={{ marginTop: 16, textAlign: "right" }}>
          <Link to="/login">
            <Trans>已有账号？登录</Trans>
          </Link>
        </div>
      </Card>
    </div>
  );
}
