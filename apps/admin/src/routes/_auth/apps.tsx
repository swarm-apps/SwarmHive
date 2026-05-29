import { PlusOutlined } from "@ant-design/icons";
import {
  DrawerForm,
  ModalForm,
  PageContainer,
  type ProColumns,
  ProFormCheckbox,
  ProFormText,
  ProTable,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { App, Button, Drawer, Empty, List, Popconfirm, Space, Tag } from "antd";
import dayjs from "dayjs";
import { useState } from "react";
import { isApiError } from "@/lib/api";
import {
  type App as AppModel,
  appsQueryOptions,
  channelsQueryOptions,
  createApp,
  createChannel,
  deleteApp,
  ERR_APP_HAS_RELEASES,
  ERR_CONFLICT,
  PLATFORMS,
  type Platform,
  platformLabel,
  setDefaultChannel,
  updateApp,
} from "@/lib/api/apps";
import { usePermissions } from "@/lib/query/usePermissions";

export const Route = createFileRoute("/_auth/apps")({
  component: AppsPage,
});

interface AppFormValues {
  slug: string;
  display_name: string;
  platforms: Platform[];
}

function AppsPage() {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();
  const { has } = usePermissions();

  const appsQuery = useQuery(appsQueryOptions());
  // 编辑 / channel 管理目标行——用受控 state 驱动两个 Drawer。
  const [editing, setEditing] = useState<AppModel | null>(null);
  const [channelsApp, setChannelsApp] = useState<AppModel | null>(null);

  const canCreate = has("app:create");
  const canUpdate = has("app:update");
  const canDelete = has("app:delete");

  const invalidateApps = () =>
    queryClient.invalidateQueries({ queryKey: appsQueryOptions().queryKey });

  async function handleCreate(values: AppFormValues): Promise<boolean> {
    try {
      await createApp({
        slug: values.slug.trim(),
        display_name: values.display_name.trim(),
        platforms: values.platforms,
      });
      notification.success({ message: t`应用已创建` });
      await invalidateApps();
      return true;
    } catch (error) {
      if (isApiError(error) && error.type === ERR_CONFLICT) {
        notification.error({ message: t`该 slug 已被占用` });
        return false;
      }
      notification.error({ message: t`创建失败，请稍后重试` });
      return false;
    }
  }

  async function handleEdit(values: AppFormValues): Promise<boolean> {
    if (!editing) return false;
    try {
      await updateApp(editing.slug, {
        display_name: values.display_name.trim(),
        platforms: values.platforms,
      });
      notification.success({ message: t`应用已更新` });
      await invalidateApps();
      setEditing(null);
      return true;
    } catch {
      notification.error({ message: t`更新失败，请稍后重试` });
      return false;
    }
  }

  async function handleDelete(app: AppModel): Promise<void> {
    try {
      await deleteApp(app.slug);
      notification.success({ message: t`应用已删除` });
      await invalidateApps();
    } catch (error) {
      if (isApiError(error) && error.type === ERR_APP_HAS_RELEASES) {
        notification.error({ message: t`该应用下仍有版本，无法删除` });
        return;
      }
      notification.error({ message: t`删除失败，请稍后重试` });
    }
  }

  const columns: ProColumns<AppModel>[] = [
    { title: t`名称`, dataIndex: "display_name" },
    { title: t`Slug`, dataIndex: "slug", copyable: true },
    {
      title: t`平台`,
      dataIndex: "platforms",
      render: (_, row) => (
        <Space size={4} wrap>
          {row.platforms.map((p) => (
            <Tag key={p} color="blue">
              {platformLabel(p)}
            </Tag>
          ))}
        </Space>
      ),
    },
    {
      title: t`创建时间`,
      dataIndex: "created_at",
      width: 180,
      render: (_, row) => dayjs(row.created_at).format("YYYY-MM-DD HH:mm"),
    },
    {
      title: t`操作`,
      width: 220,
      render: (_, row) => (
        <Space size="small">
          <Button type="link" size="small" onClick={() => setChannelsApp(row)}>
            <Trans>管理 Channel</Trans>
          </Button>
          {canUpdate && (
            <Button type="link" size="small" onClick={() => setEditing(row)}>
              <Trans>编辑</Trans>
            </Button>
          )}
          {canDelete && (
            <Popconfirm
              title={t`确认删除该应用？`}
              onConfirm={() => handleDelete(row)}
              okText={t`删除`}
              cancelText={t`取消`}
            >
              <Button type="link" size="small" danger>
                <Trans>删除</Trans>
              </Button>
            </Popconfirm>
          )}
        </Space>
      ),
    },
  ];

  return (
    <PageContainer title={t`应用`} breadcrumbRender={false}>
      <ProTable<AppModel>
        rowKey="id"
        search={false}
        options={false}
        loading={appsQuery.isPending}
        dataSource={appsQuery.data ?? []}
        pagination={{ pageSize: 20, hideOnSinglePage: true }}
        locale={{ emptyText: <Empty description={t`还没有应用，点击右上角创建第一个`} /> }}
        toolBarRender={() =>
          canCreate ? [<CreateAppDrawer key="create" onFinish={handleCreate} />] : []
        }
        columns={columns}
      />

      <EditAppDrawer
        editing={editing}
        onOpenChange={(o) => !o && setEditing(null)}
        onFinish={handleEdit}
      />

      <ChannelsDrawer
        app={channelsApp}
        canManage={canUpdate}
        onClose={() => setChannelsApp(null)}
      />
    </PageContainer>
  );
}

function CreateAppDrawer({ onFinish }: { onFinish: (v: AppFormValues) => Promise<boolean> }) {
  const { t } = useLingui();

  return (
    <DrawerForm<AppFormValues>
      title={t`创建应用`}
      trigger={
        <Button type="primary" icon={<PlusOutlined />}>
          <Trans>创建应用</Trans>
        </Button>
      }
      drawerProps={{ destroyOnClose: true }}
      onFinish={onFinish}
    >
      <ProFormText
        name="slug"
        label={t`Slug`}
        tooltip={t`唯一标识，出现在 URL 与对象路径中，创建后不可修改`}
        rules={[
          { required: true, message: t`请输入 slug` },
          {
            pattern: /^[a-z0-9][a-z0-9-]*$/,
            message: t`只能用小写字母、数字和连字符，且以字母或数字开头`,
          },
        ]}
      />
      <ProFormText
        name="display_name"
        label={t`名称`}
        rules={[{ required: true, message: t`请输入名称` }]}
      />
      <ProFormCheckbox.Group
        name="platforms"
        label={t`平台`}
        options={PLATFORMS.map((p) => ({ label: platformLabel(p), value: p }))}
        rules={[{ required: true, message: t`请至少选择一个平台` }]}
      />
    </DrawerForm>
  );
}

function EditAppDrawer({
  editing,
  onOpenChange,
  onFinish,
}: {
  editing: AppModel | null;
  onOpenChange: (open: boolean) => void;
  onFinish: (v: AppFormValues) => Promise<boolean>;
}) {
  const { t } = useLingui();

  return (
    <DrawerForm<AppFormValues>
      // key remount：切换不同行时让 initialValues 每次重新生效（见 admin-spa.md
      // 「*Form 编辑回填」坑）。slug 不可编辑，故不放进受控字段。
      key={editing?.slug ?? "none"}
      title={t`编辑应用`}
      open={editing != null}
      onOpenChange={onOpenChange}
      drawerProps={{ destroyOnClose: true }}
      initialValues={
        editing ? { display_name: editing.display_name, platforms: editing.platforms } : undefined
      }
      onFinish={onFinish}
    >
      <ProFormText
        label={t`Slug`}
        // 只读展示——slug 创建后不可变。
        fieldProps={{ value: editing?.slug, disabled: true }}
      />
      <ProFormText
        name="display_name"
        label={t`名称`}
        rules={[{ required: true, message: t`请输入名称` }]}
      />
      <ProFormCheckbox.Group
        name="platforms"
        label={t`平台`}
        options={PLATFORMS.map((p) => ({ label: platformLabel(p), value: p }))}
        rules={[{ required: true, message: t`请至少选择一个平台` }]}
      />
    </DrawerForm>
  );
}

function ChannelsDrawer({
  app,
  canManage,
  onClose,
}: {
  app: AppModel | null;
  canManage: boolean;
  onClose: () => void;
}) {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();
  const slug = app?.slug ?? "";

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
    <Drawer
      title={app ? t`管理 Channel · ${app.display_name}` : t`管理 Channel`}
      open={app != null}
      onClose={onClose}
      width={480}
      destroyOnClose
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
    </Drawer>
  );
}
