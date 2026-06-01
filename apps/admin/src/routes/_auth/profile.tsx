import { GithubOutlined } from "@ant-design/icons";
import { PageContainer } from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { App, Button, Card, List, Space, Tag, Typography } from "antd";
import { isApiError } from "@/lib/api";
import {
  identityLinksQueryOptions,
  oauthLinkStartUrl,
  publicProvidersQueryOptions,
  unlinkIdentity,
} from "@/lib/api/oauth";

export const Route = createFileRoute("/_auth/profile")({
  component: ProfilePage,
});

function ProfilePage() {
  const { t } = useLingui();
  const { notification, modal } = App.useApp();
  const queryClient = useQueryClient();

  const linksQuery = useQuery({ ...identityLinksQueryOptions(), retry: false });
  const providersQuery = useQuery(publicProvidersQueryOptions());
  const links = linksQuery.data ?? [];
  const providers = providersQuery.data ?? [];

  const githubEnabled = providers.some((p) => p.kind === "github");
  const githubLinked = links.some((l) => l.provider === "github");

  function handleUnlink(provider: string) {
    modal.confirm({
      title: t`解绑该登录方式？`,
      content: t`解绑后将无法用它登录；若这是你唯一的登录方式，请先设置密码。`,
      okText: t`解绑`,
      okButtonProps: { danger: true },
      cancelText: t`取消`,
      onOk: async () => {
        try {
          await unlinkIdentity(provider);
          notification.success({ message: t`已解绑` });
          await queryClient.invalidateQueries({
            queryKey: identityLinksQueryOptions().queryKey,
          });
        } catch (error) {
          notification.error({
            message: t`解绑失败`,
            description: isApiError(error) ? error.detail : String(error),
          });
        }
      },
    });
  }

  return (
    <PageContainer title={t`个人资料`} breadcrumbRender={false}>
      <Card title={<Trans>已绑定的登录方式</Trans>} loading={linksQuery.isPending}>
        <List
          dataSource={links}
          locale={{ emptyText: <Trans>暂无绑定的外部登录方式</Trans> }}
          renderItem={(link) => (
            <List.Item
              actions={
                link.provider === "github"
                  ? [
                      <Button
                        key="unlink"
                        type="link"
                        danger
                        onClick={() => handleUnlink(link.provider)}
                      >
                        <Trans>解绑</Trans>
                      </Button>,
                    ]
                  : []
              }
            >
              <List.Item.Meta
                avatar={link.provider === "github" ? <GithubOutlined /> : undefined}
                title={
                  <Space>
                    <Tag color="blue">{link.provider}</Tag>
                    <Typography.Text type="secondary">{link.subject}</Typography.Text>
                  </Space>
                }
                description={new Date(link.created_at).toLocaleString()}
              />
            </List.Item>
          )}
        />

        {githubEnabled && !githubLinked && (
          <Button
            icon={<GithubOutlined />}
            style={{ marginTop: 16 }}
            onClick={() => window.location.assign(oauthLinkStartUrl("github"))}
          >
            <Trans>绑定 GitHub</Trans>
          </Button>
        )}
      </Card>
    </PageContainer>
  );
}
