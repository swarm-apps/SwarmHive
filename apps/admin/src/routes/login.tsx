import { Trans, useLingui } from "@lingui/react/macro";
import { useMutation } from "@tanstack/react-query";
import { createFileRoute, Link, redirect, useRouter } from "@tanstack/react-router";
import { Alert, App, Button, Card, Checkbox, Form, Input } from "antd";
import { useState } from "react";
import { z } from "zod";
import { fetchClient, isApiError } from "@/lib/api";
import { setupInfoQueryOptions } from "@/lib/api/setup";

const searchSchema = z.object({
  next: z.string().optional(),
});

interface FormValues {
  email: string;
  password: string;
  remember?: boolean;
}

/**
 * Plain ISO-8601 → localised string. We do NOT show a live countdown
 * (would drift with server clock); displaying the absolute time lets the
 * browser localise via the user's settings.
 */
function formatLockoutUntil(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return iso;
  }
  return date.toLocaleString();
}

export const Route = createFileRoute("/login")({
  validateSearch: searchSchema,
  /**
   * Defensive: if the deployment somehow ends up at /login while still
   * in bootstrap window (e.g. user URL-typed past the root guard), bounce
   * them to /setup so the first-Owner flow is the single available door.
   */
  beforeLoad: async ({ context }) => {
    const info = await context.queryClient.ensureQueryData(setupInfoQueryOptions());
    if (info.needs_bootstrap) {
      throw redirect({ to: "/setup", replace: true });
    }
  },
  component: LoginPage,
});

function LoginPage() {
  const { t } = useLingui();
  const router = useRouter();
  const search = Route.useSearch();
  const { notification } = App.useApp();
  const [form] = Form.useForm<FormValues>();
  /** Lockout deadline (ISO string from `account-locked-until` problem). */
  const [lockoutUntil, setLockoutUntil] = useState<string | null>(null);
  /** Generic credential error banner — never names the email per spec ④. */
  const [credentialError, setCredentialError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: async (values: FormValues) => {
      const { error, response } = await fetchClient.POST("/api/v1/auth/login", {
        body: { email: values.email.trim(), password: values.password },
      });
      if (error) {
        throw error;
      }
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }
    },
    onError: (error) => {
      if (!isApiError(error)) {
        notification.error({ message: t`登录失败，请稍后重试。` });
        return;
      }
      switch (error.type) {
        case "https://swarmhive.dev/errors/account-locked-until": {
          // Server attaches `locked_until` as an extra field on the
          // problem+json body; `ApiError.extra()` reads it from the raw
          // body verbatim.
          setLockoutUntil(error.extra<string>("locked_until") ?? null);
          setCredentialError(null);
          break;
        }
        default:
          // Covers `unauthorized` (wrong password / unknown email — server
          // collapses both into the same code per spec) and any unforeseen
          // type from this endpoint. Surface as a generic credential error.
          setLockoutUntil(null);
          setCredentialError(t`邮箱或密码错误。`);
      }
    },
    onSuccess: () => {
      const next = search.next ?? "/";
      router.navigate({ to: next, replace: true });
    },
  });

  const locked = lockoutUntil !== null;

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
      <Card style={{ width: 400 }} title={<Trans>登录 SwarmHive</Trans>}>
        {locked ? (
          <Alert
            type="warning"
            showIcon
            style={{ marginBottom: 16 }}
            message={<Trans>账号已临时锁定</Trans>}
            description={
              lockoutUntil ? (
                <Trans>
                  请于 {formatLockoutUntil(lockoutUntil)} 之后重试，或通过 “忘记密码” 重置。
                </Trans>
              ) : (
                <Trans>账号已临时锁定，请稍后重试。</Trans>
              )
            }
          />
        ) : null}
        <Form<FormValues>
          form={form}
          layout="vertical"
          requiredMark={false}
          onFinish={(values) => {
            // Once a lockout fires, disable further submits until the user
            // clears the alert (refresh / next attempt window).
            if (locked) {
              return;
            }
            mutation.mutate(values);
          }}
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
            label={<Trans>密码</Trans>}
            name="password"
            help={credentialError ?? undefined}
            validateStatus={credentialError ? "error" : undefined}
            rules={[{ required: true, message: t`请输入密码` }]}
          >
            <Input.Password autoComplete="current-password" />
          </Form.Item>
          <Form.Item name="remember" valuePropName="checked" style={{ marginBottom: 12 }}>
            <Checkbox>
              <Trans>记住我</Trans>
            </Checkbox>
          </Form.Item>
          <Button
            type="primary"
            htmlType="submit"
            block
            loading={mutation.isPending}
            disabled={locked}
          >
            <Trans>登录</Trans>
          </Button>
        </Form>
        <div style={{ marginTop: 16, textAlign: "right" }}>
          <Link to="/forgot-password">
            <Trans>忘记密码？</Trans>
          </Link>
        </div>
      </Card>
    </div>
  );
}
