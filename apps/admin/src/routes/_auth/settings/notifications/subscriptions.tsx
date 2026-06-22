import { ApiOutlined, DeleteOutlined, MailOutlined, PlusOutlined } from "@ant-design/icons";
import {
  DrawerForm,
  type ProColumns,
  ProFormDependency,
  ProFormSelect,
  ProFormText,
  ProTable,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { App, Button, Popconfirm, Space, Tag } from "antd";
import dayjs from "dayjs";
import { isApiError } from "@/lib/api";
import { type App as AppEntity, appsQueryOptions } from "@/lib/api/apps";
import {
  type CreateSubscriptionReq,
  createSubscription,
  deleteSubscription,
  EVENT_TYPES,
  endpointName,
  endpointsQueryOptions,
  type NotificationChannelKind,
  type NotificationEventType,
  type Subscription,
  subscriptionsQueryOptions,
  type WebhookEndpoint,
} from "@/lib/api/notifications";

export const Route = createFileRoute("/_auth/settings/notifications/subscriptions")({
  component: SubscriptionsPage,
});

interface SubscriptionFormValues {
  event_type: NotificationEventType;
  channel_kind: NotificationChannelKind;
  email_to?: string;
  webhook_endpoint_id?: string;
  app_id?: string;
}

function SubscriptionsPage() {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();

  const subsQuery = useQuery(subscriptionsQueryOptions());
  const endpointsQuery = useQuery(endpointsQueryOptions());
  const appsQuery = useQuery(appsQueryOptions());
  const subscriptions = subsQuery.data ?? [];
  const endpoints = endpointsQuery.data ?? [];
  const apps = appsQuery.data ?? [];

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: subscriptionsQueryOptions().queryKey });

  const appLabel = (id: string): string => {
    const a = apps.find((x) => x.id === id);
    return a ? `${a.display_name} (${a.slug})` : id;
  };

  const handleCreate = async (values: SubscriptionFormValues): Promise<boolean> => {
    try {
      const body: CreateSubscriptionReq = {
        event_type: values.event_type,
        channel_kind: values.channel_kind,
        app_id: values.app_id ?? null,
        email_to: values.channel_kind === "email" ? values.email_to : null,
        webhook_endpoint_id: values.channel_kind === "webhook" ? values.webhook_endpoint_id : null,
      };
      await createSubscription(body);
      notification.success({ message: t`订阅已创建` });
      invalidate();
      return true;
    } catch (error) {
      notification.error({
        message: t`创建失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
      return false;
    }
  };

  const handleDelete = async (row: Subscription) => {
    try {
      await deleteSubscription(row.id);
      notification.success({ message: t`订阅已删除` });
      invalidate();
    } catch (error) {
      notification.error({
        message: t`删除失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
    }
  };

  const columns: ProColumns<Subscription>[] = [
    {
      title: t`事件`,
      dataIndex: "event_type",
      render: (_, row) => <Tag color="blue">{row.event_type}</Tag>,
    },
    {
      title: t`通道`,
      render: (_, row) =>
        row.channel_kind === "email" ? (
          <Space size={4}>
            <MailOutlined />
            {row.email_to}
          </Space>
        ) : (
          <Space size={4}>
            <ApiOutlined />
            {endpointName(endpoints, row.webhook_endpoint_id) ?? row.webhook_endpoint_id}
          </Space>
        ),
    },
    {
      title: t`应用范围`,
      render: (_, row) =>
        row.app_id ? (
          appLabel(row.app_id)
        ) : (
          <Tag>
            <Trans>所有应用</Trans>
          </Tag>
        ),
    },
    {
      title: t`创建时间`,
      dataIndex: "created_at",
      width: 170,
      render: (_, row) => dayjs(row.created_at).format("YYYY-MM-DD HH:mm"),
    },
    {
      title: t`操作`,
      width: 90,
      render: (_, row) => (
        <Popconfirm
          title={t`删除该订阅？`}
          okButtonProps={{ danger: true }}
          onConfirm={() => handleDelete(row)}
        >
          <Button type="link" size="small" danger icon={<DeleteOutlined />}>
            <Trans>删除</Trans>
          </Button>
        </Popconfirm>
      ),
    },
  ];

  return (
    <ProTable<Subscription>
      rowKey="id"
      search={false}
      options={false}
      loading={subsQuery.isPending}
      dataSource={subscriptions}
      pagination={{ pageSize: 20, hideOnSinglePage: true }}
      toolBarRender={() => [
        <CreateSubscriptionDrawer
          key="add"
          endpoints={endpoints}
          apps={apps}
          onFinish={handleCreate}
        />,
      ]}
      columns={columns}
    />
  );
}

function CreateSubscriptionDrawer({
  endpoints,
  apps,
  onFinish,
}: {
  endpoints: WebhookEndpoint[];
  apps: AppEntity[];
  onFinish: (v: SubscriptionFormValues) => Promise<boolean>;
}) {
  const { t } = useLingui();
  return (
    <DrawerForm<SubscriptionFormValues>
      title={t`新建订阅`}
      trigger={
        <Button type="primary" icon={<PlusOutlined />}>
          <Trans>新建订阅</Trans>
        </Button>
      }
      drawerProps={{ destroyOnClose: true }}
      initialValues={{ channel_kind: "email" }}
      onFinish={onFinish}
    >
      <ProFormSelect
        name="event_type"
        label={t`事件类型`}
        options={EVENT_TYPES.map((e) => ({ label: e, value: e }))}
        rules={[{ required: true, message: t`必选` }]}
      />
      <ProFormSelect
        name="channel_kind"
        label={t`通道类型`}
        options={[
          { label: t`Email`, value: "email" },
          { label: t`Webhook`, value: "webhook" },
        ]}
        rules={[{ required: true, message: t`必选` }]}
      />
      <ProFormDependency name={["channel_kind"]}>
        {({ channel_kind }) =>
          channel_kind === "webhook" ? (
            <ProFormSelect
              name="webhook_endpoint_id"
              label={t`Webhook Endpoint`}
              options={endpoints.map((e) => ({ label: e.name, value: e.id }))}
              rules={[{ required: true, message: t`必选` }]}
            />
          ) : (
            <ProFormText
              name="email_to"
              label={t`收件地址`}
              rules={[
                { required: true, message: t`必填` },
                { type: "email", message: t`邮箱格式不正确` },
              ]}
            />
          )
        }
      </ProFormDependency>
      <ProFormSelect
        name="app_id"
        label={t`应用范围（可选）`}
        tooltip={t`留空则匹配所有应用`}
        allowClear
        placeholder={t`所有应用`}
        options={apps.map((a) => ({ label: `${a.display_name} (${a.slug})`, value: a.id }))}
      />
    </DrawerForm>
  );
}
