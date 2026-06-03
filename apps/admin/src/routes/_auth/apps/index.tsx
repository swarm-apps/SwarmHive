import { PlusOutlined } from "@ant-design/icons";
import {
  DrawerForm,
  PageContainer,
  type ProColumns,
  ProFormCheckbox,
  ProFormText,
  ProTable,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { App, Button, Empty, Space, Tag } from "antd";
import dayjs from "dayjs";
import { isApiError } from "@/lib/api";
import {
  type App as AppModel,
  appsQueryOptions,
  createApp,
  ERR_CONFLICT,
  PLATFORMS,
  type Platform,
  platformLabel,
} from "@/lib/api/apps";
import { usePermissions } from "@/lib/query/usePermissions";

export const Route = createFileRoute("/_auth/apps/")({
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
  const navigate = useNavigate();

  const appsQuery = useQuery(appsQueryOptions());
  const canCreate = has("app:create");

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
      width: 120,
      render: (_, row) => (
        <Button
          type="link"
          size="small"
          onClick={() => navigate({ to: "/apps/$slug", params: { slug: row.slug } })}
        >
          <Trans>进入</Trans>
        </Button>
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
