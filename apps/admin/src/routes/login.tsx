import { Trans, useLingui } from "@lingui/react/macro";
import { createFileRoute } from "@tanstack/react-router";
import { Alert, Card, Form, Input } from "antd";
import { z } from "zod";

const searchSchema = z.object({
  next: z.string().optional(),
});

export const Route = createFileRoute("/login")({
  validateSearch: searchSchema,
  component: LoginPage,
});

function LoginPage() {
  const { t } = useLingui();
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
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 16 }}
          message={<Trans>登录表单尚未实现，将由后续 auth UI proposal 接管。</Trans>}
        />
        <Form layout="vertical" disabled>
          <Form.Item label={<Trans>邮箱</Trans>} name="email">
            <Input placeholder={t`you@example.com`} />
          </Form.Item>
          <Form.Item label={<Trans>密码</Trans>} name="password">
            <Input.Password />
          </Form.Item>
        </Form>
      </Card>
    </div>
  );
}
