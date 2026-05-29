import { PageContainer, ProCard, ProDescriptions } from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { Button, Space, Tag } from "antd";
import dayjs from "dayjs";
import { meQueryOptions } from "@/lib/query/meQuery";
import { useResendVerify } from "@/lib/query/useResendVerify";

export const Route = createFileRoute("/_auth/settings/account")({
  component: AccountPage,
});

function AccountPage() {
  const { t } = useLingui();
  const me = useQuery({ ...meQueryOptions(), retry: false });
  const resend = useResendVerify();

  const user = me.data?.user;
  const verified = user?.email_verified_at != null;

  return (
    <PageContainer title={t`账户`} breadcrumbRender={false}>
      <ProCard title={<Trans>账户信息</Trans>} loading={me.isPending}>
        <ProDescriptions column={1}>
          <ProDescriptions.Item label={t`邮箱`}>{user?.email}</ProDescriptions.Item>
          <ProDescriptions.Item label={t`显示名称`}>{user?.display_name}</ProDescriptions.Item>
          <ProDescriptions.Item label={t`邮箱验证状态`}>
            {verified ? (
              <Tag color="success">
                <Trans>已验证</Trans>
              </Tag>
            ) : (
              <Space>
                <Tag color="default">
                  <Trans>未验证</Trans>
                </Tag>
                <Button
                  size="small"
                  type="link"
                  loading={resend.isPending}
                  onClick={() => resend.mutate()}
                >
                  <Trans>重发验证邮件</Trans>
                </Button>
              </Space>
            )}
          </ProDescriptions.Item>
          {verified && user?.email_verified_at ? (
            <ProDescriptions.Item label={t`验证时间`}>
              {dayjs(user.email_verified_at).format("YYYY-MM-DD HH:mm")}
            </ProDescriptions.Item>
          ) : null}
        </ProDescriptions>
      </ProCard>
    </PageContainer>
  );
}
