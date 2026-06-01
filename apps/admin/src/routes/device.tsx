import { Trans, useLingui } from "@lingui/react/macro";
import { useMutation, useQuery } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { App, Button, Card, Form, Input, Result, Space, Spin, Typography } from "antd";
import { useState } from "react";
import { z } from "zod";
import { isApiError } from "@/lib/api";
import { deviceLookupQueryOptions, postDeviceApprove, postDeviceDeny } from "@/lib/api/device";
import { meQueryOptions } from "@/lib/query/meQuery";

const searchSchema = z.object({
  user_code: z.string().optional(),
});

export const Route = createFileRoute("/device")({
  validateSearch: searchSchema,
  component: DevicePage,
});

/**
 * RFC 8628 device-approval page. Public top-level route (NOT under `_auth`):
 * the `_auth` guard redirects with `next: location.pathname`, which would drop
 * the `?user_code` query. Here we gate on `/me` ourselves and, when signed out,
 * link to `/login` with a `next` that carries the code so it survives the
 * round-trip — inheriting whatever sign-in methods `/login` offers (password
 * today, GitHub once add-oauth-github lands).
 */
function DevicePage() {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const search = Route.useSearch();
  const [code, setCode] = useState((search.user_code ?? "").toUpperCase());
  const [resolved, setResolved] = useState<"approved" | "denied" | null>(null);

  const me = useQuery({ ...meQueryOptions(), retry: false });

  const lookup = useQuery({
    ...deviceLookupQueryOptions(code),
    enabled: me.isSuccess && code.length > 0,
  });

  const approve = useMutation({
    mutationFn: () => postDeviceApprove(code),
    onSuccess: () => setResolved("approved"),
    onError: () => notification.error({ message: t`授权失败，请确认验证码是否正确或已过期。` }),
  });
  const deny = useMutation({
    mutationFn: () => postDeviceDeny(code),
    onSuccess: () => setResolved("denied"),
    onError: () => notification.error({ message: t`操作失败，请稍后重试。` }),
  });

  if (me.isPending) {
    return (
      <PageShell>
        <Spin />
      </PageShell>
    );
  }

  // Signed out → prompt to sign in, preserving the code through the redirect.
  if (me.isError) {
    const next = code ? `/device?user_code=${encodeURIComponent(code)}` : "/device";
    return (
      <PageShell>
        <Card style={{ width: 460 }} title={<Trans>授权命令行登录</Trans>}>
          <Typography.Paragraph type="secondary">
            <Trans>请先登录，以授权命令行设备访问你的账号。</Trans>
          </Typography.Paragraph>
          {code ? (
            <Typography.Paragraph>
              <Trans>验证码</Trans>： <Typography.Text strong>{code}</Typography.Text>
            </Typography.Paragraph>
          ) : null}
          <Link to="/login" search={{ next }}>
            <Button type="primary" block>
              <Trans>前往登录</Trans>
            </Button>
          </Link>
        </Card>
      </PageShell>
    );
  }

  // Resolved → tell the user to return to the terminal.
  if (resolved) {
    return (
      <PageShell>
        <Card style={{ width: 460 }}>
          <Result
            status={resolved === "approved" ? "success" : "warning"}
            title={resolved === "approved" ? <Trans>已授权</Trans> : <Trans>已拒绝</Trans>}
            subTitle={
              resolved === "approved" ? (
                <Trans>命令行登录已获授权，请回到终端继续。</Trans>
              ) : (
                <Trans>已拒绝该登录请求。如非你本人发起，请放心忽略。</Trans>
              )
            }
          />
        </Card>
      </PageShell>
    );
  }

  const lookupFailed = lookup.isError;
  const grant = lookup.data;

  return (
    <PageShell>
      <Card style={{ width: 460 }} title={<Trans>授权命令行登录</Trans>}>
        <Form layout="vertical" onFinish={() => lookup.refetch()}>
          <Form.Item label={<Trans>验证码</Trans>}>
            <Input
              placeholder="WDJB-MJHT"
              value={code}
              onChange={(e) => setCode(e.target.value.toUpperCase())}
              autoFocus
            />
          </Form.Item>
        </Form>

        {lookupFailed && isApiError(lookup.error) ? (
          <Result
            status="error"
            title={<Trans>验证码无效或已过期</Trans>}
            subTitle={<Trans>请确认终端显示的验证码，或重新运行 swarmhive login。</Trans>}
          />
        ) : null}

        {grant ? (
          <>
            <Typography.Paragraph>
              <Trans>
                <Typography.Text strong>{grant.client_name ?? grant.client_id}</Typography.Text>{" "}
                正在请求访问你的账号。仅当这是你刚刚运行的 swarmhive login 时才批准。
              </Trans>
            </Typography.Paragraph>
            <Space>
              <Button type="primary" loading={approve.isPending} onClick={() => approve.mutate()}>
                <Trans>批准</Trans>
              </Button>
              <Button danger loading={deny.isPending} onClick={() => deny.mutate()}>
                <Trans>拒绝</Trans>
              </Button>
            </Space>
          </>
        ) : null}
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
