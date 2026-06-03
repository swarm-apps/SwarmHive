import { PlusOutlined } from "@ant-design/icons";
import { ModalForm, ProFormSelect, ProFormText } from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { App, Button, Card, Empty, List, Popconfirm, Space, Tag } from "antd";
import { isApiError } from "@/lib/api";
import {
  channelsQueryOptions,
  createChannel,
  ERR_CONFLICT,
  setDefaultChannel,
} from "@/lib/api/apps";
import { ERR_NOTHING_TO_ROLLBACK } from "@/lib/api/errors";
import {
  channelReleaseQueryOptions,
  promote,
  publishedVersions,
  type Release,
  releasesQueryOptions,
  rollback,
} from "@/lib/api/releases";
import { usePermissions } from "@/lib/query/usePermissions";

export const Route = createFileRoute("/_auth/apps/$slug/channels")({
  component: ChannelsTab,
});

function ChannelsTab() {
  const { slug } = Route.useParams();
  const { has } = usePermissions();
  const releasesQuery = useQuery(releasesQueryOptions(slug));
  const published = publishedVersions(releasesQuery.data ?? []);

  return (
    <Space direction="vertical" size="large" style={{ width: "100%" }}>
      <ChannelConfig slug={slug} canManage={has("app:update")} />
      <ReleaseTrainPanel slug={slug} published={published} has={has} />
    </Space>
  );
}

// channel 配置（列表 / 创建 / 设默认）—— 原 apps 页 ChannelsDrawer 内联到渠道 tab 的卡片。
function ChannelConfig({ slug, canManage }: { slug: string; canManage: boolean }) {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();
  const channelsQuery = useQuery(channelsQueryOptions(slug));

  const invalidateChannels = () =>
    queryClient.invalidateQueries({ queryKey: channelsQueryOptions(slug).queryKey });

  async function handleSetDefault(name: string): Promise<void> {
    try {
      await setDefaultChannel(slug, name);
      notification.success({ message: t`已设为默认 Channel` });
      await invalidateChannels();
    } catch {
      notification.error({ message: t`设置失败，请稍后重试` });
    }
  }

  async function handleCreate(values: { name: string }): Promise<boolean> {
    try {
      await createChannel(slug, { name: values.name.trim() });
      notification.success({ message: t`Channel 已创建` });
      await invalidateChannels();
      return true;
    } catch (error) {
      if (isApiError(error) && error.type === ERR_CONFLICT) {
        notification.error({ message: t`该 Channel 名已存在` });
        return false;
      }
      notification.error({ message: t`创建失败，请稍后重试` });
      return false;
    }
  }

  return (
    <Card
      title={t`Channel`}
      size="small"
      extra={
        canManage ? (
          <ModalForm<{ name: string }>
            title={t`添加 Channel`}
            trigger={
              <Button type="primary" size="small" icon={<PlusOutlined />}>
                <Trans>添加 Channel</Trans>
              </Button>
            }
            modalProps={{ destroyOnClose: true }}
            width={360}
            onFinish={handleCreate}
          >
            <ProFormText
              name="name"
              label={t`名称`}
              rules={[
                { required: true, message: t`请输入 Channel 名` },
                {
                  pattern: /^[a-z0-9][a-z0-9-]*$/,
                  message: t`只能用小写字母、数字和连字符`,
                },
              ]}
            />
          </ModalForm>
        ) : null
      }
    >
      <List
        loading={channelsQuery.isPending}
        dataSource={channelsQuery.data ?? []}
        locale={{ emptyText: <Empty description={t`暂无 Channel`} /> }}
        renderItem={(ch) => (
          <List.Item
            actions={
              canManage && !ch.is_default
                ? [
                    <Button
                      key="default"
                      type="link"
                      size="small"
                      onClick={() => handleSetDefault(ch.name)}
                    >
                      <Trans>设为默认</Trans>
                    </Button>,
                  ]
                : []
            }
          >
            <Space>
              <span>{ch.name}</span>
              {ch.is_default && (
                <Tag color="green">
                  <Trans>默认</Trans>
                </Tag>
              )}
            </Space>
          </List.Item>
        )}
      />
    </Card>
  );
}

function ReleaseTrainPanel({
  slug,
  published,
  has,
}: {
  slug: string;
  published: Release[];
  has: (p: Parameters<ReturnType<typeof usePermissions>["has"]>[0]) => boolean;
}) {
  const { t } = useLingui();
  const channelsQuery = useQuery(channelsQueryOptions(slug));
  const channels = channelsQuery.data ?? [];

  return (
    <Card title={t`发布列车`} size="small" loading={channelsQuery.isPending}>
      <List
        dataSource={channels}
        locale={{ emptyText: <Empty description={t`暂无 Channel`} /> }}
        renderItem={(ch) => (
          <ChannelPointerRow
            slug={slug}
            channel={ch.name}
            isDefault={ch.is_default}
            published={published}
            canPromote={has("release:promote")}
            canRollback={has("release:rollback")}
          />
        )}
      />
    </Card>
  );
}

function ChannelPointerRow({
  slug,
  channel,
  isDefault,
  published,
  canPromote,
  canRollback,
}: {
  slug: string;
  channel: string;
  isDefault: boolean;
  published: Release[];
  canPromote: boolean;
  canRollback: boolean;
}) {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();
  const pointerQuery = useQuery(channelReleaseQueryOptions(slug, channel));
  const current = pointerQuery.data ?? null;

  const invalidatePointer = () =>
    queryClient.invalidateQueries({
      queryKey: channelReleaseQueryOptions(slug, channel).queryKey,
    });

  async function handlePromote(values: { version: string }): Promise<boolean> {
    try {
      await promote(slug, channel, { version: values.version });
      notification.success({ message: t`已 promote 到 ${channel}` });
      await invalidatePointer();
      return true;
    } catch (error) {
      if (isApiError(error) && error.type === ERR_CONFLICT) {
        notification.error({ message: t`只能 promote 已发布的版本` });
        return false;
      }
      notification.error({ message: t`promote 失败，请稍后重试` });
      return false;
    }
  }

  async function handleRollback(): Promise<void> {
    try {
      await rollback(slug, channel, {});
      notification.success({ message: t`已回滚 ${channel}` });
      await invalidatePointer();
    } catch (error) {
      if (isApiError(error) && error.type === ERR_NOTHING_TO_ROLLBACK) {
        notification.error({ message: t`无可回滚版本` });
        return;
      }
      notification.error({ message: t`回滚失败，请稍后重试` });
    }
  }

  return (
    <List.Item
      actions={[
        ...(canPromote
          ? [
              <ModalForm<{ version: string }>
                key="promote"
                title={t`promote 到 ${channel}`}
                trigger={
                  <Button type="link" size="small">
                    <Trans>promote</Trans>
                  </Button>
                }
                modalProps={{ destroyOnClose: true }}
                width={360}
                onFinish={handlePromote}
              >
                <ProFormSelect
                  name="version"
                  label={t`已发布版本`}
                  options={published.map((r) => ({ label: r.version, value: r.version }))}
                  rules={[{ required: true, message: t`请选择版本` }]}
                />
              </ModalForm>,
            ]
          : []),
        ...(canRollback
          ? [
              <Popconfirm
                key="rollback"
                title={t`回滚 ${channel} 到上一个版本？`}
                onConfirm={handleRollback}
                okText={t`回滚`}
                cancelText={t`取消`}
              >
                <Button type="link" size="small" danger>
                  <Trans>回滚</Trans>
                </Button>
              </Popconfirm>,
            ]
          : []),
      ]}
    >
      <Space>
        <strong>{channel}</strong>
        {isDefault && (
          <Tag color="green">
            <Trans>默认</Trans>
          </Tag>
        )}
        <span style={{ color: "rgba(0,0,0,0.45)" }}>
          {pointerQuery.isPending ? "…" : (current?.version ?? t`未指向版本`)}
        </span>
      </Space>
    </List.Item>
  );
}
