import {
  DeleteOutlined,
  EditOutlined,
  PlusOutlined,
  ReloadOutlined,
  SendOutlined,
} from "@ant-design/icons";
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
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import {
  Alert,
  App,
  Button,
  Modal,
  Popconfirm,
  Space,
  Switch,
  Tag,
  Tooltip,
  Typography,
} from "antd";
import dayjs from "dayjs";
import { useState } from "react";
import { isApiError } from "@/lib/api";
import {
  countSubscriptionsForEndpoint,
  createEndpoint,
  deleteEndpoint,
  endpointsQueryOptions,
  rotateSecret,
  subscriptionsQueryOptions,
  testEndpoint,
  updateEndpoint,
  type WebhookEndpoint,
  type WebhookProviderKind,
} from "@/lib/api/notifications";

export const Route = createFileRoute("/_auth/settings/notifications/")({
  component: EndpointsPage,
});

interface EndpointFormValues {
  name: string;
  url: string;
  provider_kind?: WebhookProviderKind;
  secret?: string;
}

const PROVIDER_OPTIONS = [
  { label: "Generic (Standard Webhooks)", value: "generic" },
  { label: "飞书 / Lark", value: "feishu" },
  { label: "Slack", value: "slack" },
  { label: "钉钉 / DingTalk", value: "dingtalk" },
  { label: "Discord", value: "discord" },
];

function EndpointsPage() {
  const { t } = useLingui();
  const { notification, modal } = App.useApp();
  const queryClient = useQueryClient();
  const navigate = useNavigate();

  const endpointsQuery = useQuery(endpointsQueryOptions());
  const subsQuery = useQuery(subscriptionsQueryOptions());
  const endpoints = endpointsQuery.data ?? [];
  const subscriptions = subsQuery.data ?? [];

  const [editing, setEditing] = useState<WebhookEndpoint | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  // 一次性密钥展示:创建 / 轮换后弹窗,关闭即不可再见(后端只在 create/rotate 返回明文)。
  const [revealed, setRevealed] = useState<{ title: string; secret: string } | null>(null);

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: endpointsQueryOptions().queryKey });
    // 删除 endpoint 会级联删订阅 → 同步刷新订阅缓存,让「订阅数」列准确。
    queryClient.invalidateQueries({ queryKey: subscriptionsQueryOptions().queryKey });
  };

  const handleSubmit = async (values: EndpointFormValues): Promise<boolean> => {
    try {
      if (editing) {
        await updateEndpoint(editing.id, { name: values.name, url: values.url });
        notification.success({ message: t`Endpoint 已更新` });
      } else {
        const created = await createEndpoint({
          name: values.name,
          url: values.url,
          provider_kind: values.provider_kind ?? "generic",
          secret: values.secret || null,
        });
        // generic 返回 whsec_ 一次性揭示;IM provider 无 SwarmHive 生成的密钥。
        if (created.secret) {
          setRevealed({ title: t`Endpoint 已创建`, secret: created.secret });
        } else {
          notification.success({ message: t`Endpoint 已创建` });
        }
      }
      setDrawerOpen(false);
      setEditing(null);
      invalidate();
      return true;
    } catch (error) {
      notification.error({
        message: t`保存失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
      return false;
    }
  };

  const handleToggleDisabled = async (row: WebhookEndpoint, disabled: boolean) => {
    const key = endpointsQueryOptions().queryKey;
    const previous = queryClient.getQueryData<WebhookEndpoint[]>(key);
    // 乐观更新:开关立即翻转,不必等 PATCH 往返 + refetch;失败回滚。
    queryClient.setQueryData<WebhookEndpoint[]>(key, (old) =>
      old?.map((e) => (e.id === row.id ? { ...e, disabled } : e)),
    );
    try {
      await updateEndpoint(row.id, { disabled });
      queryClient.invalidateQueries({ queryKey: key });
    } catch (error) {
      queryClient.setQueryData(key, previous);
      notification.error({
        message: t`更新失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
    }
  };

  const handleTest = async (row: WebhookEndpoint) => {
    try {
      const resp = await testEndpoint(row.id);
      if (resp.ok) {
        notification.success({
          message: t`测试请求成功`,
          description: resp.response_code ? t`响应码：${resp.response_code}` : resp.detail,
        });
      } else {
        notification.warning({ message: t`测试请求失败`, description: resp.detail });
      }
    } catch (error) {
      notification.error({
        message: t`测试失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
    }
  };

  const handleRotate = (row: WebhookEndpoint) => {
    modal.confirm({
      title: t`轮换签名密钥？`,
      // 后端实现了 Standard Webhooks dual-signing:旧密钥保留 24h 双签,零停机。
      content: t`旧密钥会保留 24 小时，期间投递同时用新旧密钥双签，接收端可从容切换到新密钥。`,
      okText: t`轮换`,
      okButtonProps: { danger: true },
      onOk: async () => {
        try {
          const resp = await rotateSecret(row.id);
          setRevealed({ title: t`新签名密钥`, secret: resp.secret });
        } catch (error) {
          notification.error({
            message: t`轮换失败`,
            description: isApiError(error) ? error.detail : String(error),
          });
        }
      },
    });
  };

  const handleDelete = async (row: WebhookEndpoint) => {
    try {
      await deleteEndpoint(row.id);
      notification.success({ message: t`已删除`, description: row.name });
      invalidate();
    } catch (error) {
      notification.error({
        message: t`删除失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
    }
  };

  const columns: ProColumns<WebhookEndpoint>[] = [
    {
      title: t`名称`,
      dataIndex: "name",
      render: (_, row) => (
        <Space>
          {row.name}
          <HealthTag endpoint={row} />
          <GraceTag endpoint={row} />
        </Space>
      ),
    },
    {
      title: t`Provider`,
      width: 110,
      render: (_, row) =>
        row.provider_kind === "generic" ? (
          <span style={{ color: "rgba(0,0,0,0.45)" }}>generic</span>
        ) : (
          <Tag color="purple">{row.provider_kind}</Tag>
        ),
    },
    {
      title: t`URL`,
      dataIndex: "url",
      ellipsis: true,
      render: (_, row) => <code style={{ fontFamily: "monospace" }}>{row.url}</code>,
    },
    {
      title: t`状态`,
      width: 110,
      render: (_, row) => (
        <Switch
          checked={!row.disabled}
          checkedChildren={t`启用`}
          unCheckedChildren={t`停用`}
          onChange={(checked) => handleToggleDisabled(row, !checked)}
        />
      ),
    },
    {
      title: t`订阅数`,
      width: 90,
      render: (_, row) => countSubscriptionsForEndpoint(subscriptions, row.id),
    },
    {
      title: t`创建时间`,
      dataIndex: "created_at",
      width: 170,
      render: (_, row) => dayjs(row.created_at).format("YYYY-MM-DD HH:mm"),
    },
    {
      title: t`操作`,
      width: 340,
      render: (_, row) => {
        const used = countSubscriptionsForEndpoint(subscriptions, row.id);
        return (
          <Space wrap>
            <Button
              size="small"
              onClick={() =>
                navigate({
                  to: "/settings/notifications/deliveries",
                  search: { endpoint: row.id },
                })
              }
            >
              <Trans>查看投递</Trans>
            </Button>
            <Button
              size="small"
              icon={<EditOutlined />}
              onClick={() => {
                setEditing(row);
                setDrawerOpen(true);
              }}
            >
              <Trans>编辑</Trans>
            </Button>
            <Button size="small" icon={<SendOutlined />} onClick={() => handleTest(row)}>
              <Trans>测试</Trans>
            </Button>
            <Button size="small" icon={<ReloadOutlined />} onClick={() => handleRotate(row)}>
              <Trans>轮换密钥</Trans>
            </Button>
            <Popconfirm
              title={t`删除该 Endpoint？`}
              description={
                used > 0 ? t`将同时删除指向它的 ${used} 条订阅，且不可撤销。` : t`该操作不可撤销。`
              }
              okButtonProps={{ danger: true }}
              onConfirm={() => handleDelete(row)}
            >
              <Button size="small" danger icon={<DeleteOutlined />}>
                <Trans>删除</Trans>
              </Button>
            </Popconfirm>
          </Space>
        );
      },
    },
  ];

  return (
    <>
      <ProTable<WebhookEndpoint>
        rowKey="id"
        search={false}
        options={false}
        loading={endpointsQuery.isPending}
        dataSource={endpoints}
        pagination={{ pageSize: 20, hideOnSinglePage: true }}
        toolBarRender={() => [
          <Button
            key="add"
            type="primary"
            icon={<PlusOutlined />}
            onClick={() => {
              setEditing(null);
              setDrawerOpen(true);
            }}
          >
            <Trans>新建 Endpoint</Trans>
          </Button>,
        ]}
        columns={columns}
      />

      {/* key 随 editing 变化强制 remount:DrawerForm 的 initialValues 只首次挂载生效。 */}
      <DrawerForm<EndpointFormValues>
        key={editing?.id ?? "new"}
        title={editing ? t`编辑 Endpoint` : t`新建 Endpoint`}
        open={drawerOpen}
        onOpenChange={(open) => {
          setDrawerOpen(open);
          if (!open) {
            setEditing(null);
          }
        }}
        initialValues={editing ? { name: editing.name, url: editing.url } : undefined}
        onFinish={handleSubmit}
      >
        <ProFormText name="name" label={t`名称`} rules={[{ required: true, message: t`必填` }]} />
        <ProFormText
          name="url"
          label={t`URL`}
          tooltip={t`Slack / 飞书 / Discord：粘贴其 incoming webhook URL 即可`}
          rules={[
            { required: true, message: t`必填` },
            { type: "url", message: t`URL 格式不正确` },
            {
              // 对齐后端 validate_webhook_url:release 构建强制 https,dev 放行 http。
              validator: (_, value: string | undefined) => {
                if (!value) return Promise.resolve();
                const httpAllowed = import.meta.env.DEV && value.startsWith("http://");
                if (value.startsWith("https://") || httpAllowed) {
                  return Promise.resolve();
                }
                return Promise.reject(new Error(t`Webhook URL 必须使用 https`));
              },
            },
          ]}
        />
        {/* provider 仅创建时可选(创建后不可改);IM 加签密钥按 provider 条件显示。 */}
        {!editing && (
          <>
            <ProFormSelect
              name="provider_kind"
              label={t`Provider`}
              initialValue="generic"
              options={PROVIDER_OPTIONS}
              tooltip={t`generic 走 Standard Webhooks;IM 平台产出原生消息体。`}
            />
            <ProFormDependency name={["provider_kind"]}>
              {({ provider_kind }) =>
                provider_kind === "feishu" || provider_kind === "dingtalk" ? (
                  <ProFormText.Password
                    name="secret"
                    label={t`加签密钥（可选）`}
                    tooltip={t`飞书 / 钉钉的加签密钥;留空则不加签（需用关键词或 IP 白名单保证安全）。`}
                  />
                ) : null
              }
            </ProFormDependency>
          </>
        )}
      </DrawerForm>

      <SecretRevealModal reveal={revealed} onClose={() => setRevealed(null)} />
    </>
  );
}

/** 失败健康标签:`disabled && failing_since` → 因连续失败自动停用(红);仅 failing_since → 连续失败中(橙)。 */
function HealthTag({ endpoint }: { endpoint: WebhookEndpoint }) {
  const { t } = useLingui();
  const since = endpoint.failing_since;
  if (!since) {
    return null;
  }
  const sinceText = dayjs(since).format("YYYY-MM-DD HH:mm");
  if (endpoint.disabled) {
    return (
      <Tooltip title={t`自 ${sinceText} 起连续失败，已自动停用；重新启用即恢复投递。`}>
        <Tag color="error">
          <Trans>因连续失败自动停用</Trans>
        </Tag>
      </Tooltip>
    );
  }
  return (
    <Tooltip title={t`自 ${sinceText} 起连续失败，持续失败超过 3 天将自动停用。`}>
      <Tag color="warning">
        <Trans>连续失败中</Trans>
      </Tag>
    </Tooltip>
  );
}

/** 轮换宽限期内(旧密钥未过期)展示「轮换中」标签 + 双签到期时间。 */
function GraceTag({ endpoint }: { endpoint: WebhookEndpoint }) {
  const { t } = useLingui();
  const expires = endpoint.previous_secret_expires_at;
  if (!expires || dayjs(expires).isBefore(dayjs())) {
    return null;
  }
  return (
    <Tooltip title={t`旧密钥双签至 ${dayjs(expires).format("YYYY-MM-DD HH:mm")}`}>
      <Tag color="processing">
        <Trans>轮换中</Trans>
      </Tag>
    </Tooltip>
  );
}

function SecretRevealModal({
  reveal,
  onClose,
}: {
  reveal: { title: string; secret: string } | null;
  onClose: () => void;
}) {
  const { t } = useLingui();
  return (
    <Modal
      title={reveal?.title ?? t`签名密钥`}
      open={reveal != null}
      onCancel={onClose}
      onOk={onClose}
      okText={t`我已保存`}
      cancelButtonProps={{ style: { display: "none" } }}
      destroyOnClose
    >
      <Alert
        type="warning"
        showIcon
        message={t`请立即复制保存。关闭后将无法再次查看完整密钥。`}
        style={{ marginBottom: 12 }}
      />
      <Typography.Paragraph
        copyable={{ text: reveal?.secret }}
        code
        style={{ wordBreak: "break-all" }}
      >
        {reveal?.secret}
      </Typography.Paragraph>
      <Typography.Paragraph type="secondary">
        <Trans>接收端用它验证 Standard Webhooks 签名（webhook-signature 头）。</Trans>
      </Typography.Paragraph>
    </Modal>
  );
}
