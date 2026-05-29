import { PlusOutlined } from "@ant-design/icons";
import {
  DrawerForm,
  PageContainer,
  type ProColumns,
  ProFormDatePicker,
  ProFormDependency,
  ProFormRadio,
  ProFormSelect,
  ProFormText,
  ProTable,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { Alert, App, Button, Empty, Modal, Popconfirm, Space, Tag, Typography } from "antd";
import dayjs from "dayjs";
import { type ReactNode, useState } from "react";
import { isApiError } from "@/lib/api";
import {
  ALL_PERMISSIONS,
  type ApiToken,
  type ApiTokenKind,
  type CreateTokenRequest,
  type CreateTokenResponse,
  createToken,
  type PermissionName,
  permissionLabel,
  revokeToken,
  type TokenStatus,
  tokenStatus,
  tokenStatusColor,
  tokensQueryOptions,
} from "@/lib/api/tokens";
import { usePermissions } from "@/lib/query/usePermissions";

export const Route = createFileRoute("/_auth/tokens")({
  component: TokensPage,
});

interface CreateTokenValues {
  kind: ApiTokenKind;
  name: string;
  permissions?: PermissionName[];
  expires_at?: string;
}

function TokensPage() {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();
  const { has } = usePermissions();

  const tokensQuery = useQuery(tokensQueryOptions());
  const tokens = tokensQuery.data ?? [];

  const [revealed, setRevealed] = useState<CreateTokenResponse | null>(null);

  const canManage = has("token:manage");

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: tokensQueryOptions().queryKey });

  async function handleCreate(values: CreateTokenValues): Promise<boolean> {
    try {
      const body: CreateTokenRequest = {
        kind: values.kind,
        name: values.name.trim(),
        // PAT 继承本人实时权限(permissions=null);API 用勾选的子集。
        permissions: values.kind === "api" ? (values.permissions ?? []) : null,
        expires_at: values.expires_at ? dayjs(values.expires_at).toISOString() : null,
      };
      const created = await createToken(body);
      setRevealed(created); // 明文一次性弹窗展示
      await invalidate();
      return true;
    } catch (error) {
      notification.error({
        message: t`创建失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
      return false;
    }
  }

  async function handleRevoke(id: string): Promise<void> {
    try {
      await revokeToken(id);
      notification.success({ message: t`令牌已撤销` });
      await invalidate();
    } catch (error) {
      notification.error({
        message: t`撤销失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
    }
  }

  const columns: ProColumns<ApiToken>[] = [
    { title: t`名称`, dataIndex: "name" },
    {
      title: t`前缀`,
      dataIndex: "prefix",
      width: 150,
      render: (_, row) => <code style={{ fontFamily: "monospace" }}>{row.prefix}</code>,
    },
    {
      title: t`类型`,
      dataIndex: "kind",
      width: 80,
      render: (_, row) => (row.kind === "pat" ? <Tag>PAT</Tag> : <Tag color="blue">API</Tag>),
    },
    {
      title: t`权限`,
      render: (_, row) =>
        row.kind === "pat" ? (
          <span style={{ color: "rgba(0,0,0,0.45)" }}>
            <Trans>继承本人全部权限</Trans>
          </span>
        ) : (
          <Space size={[0, 4]} wrap>
            {(row.permissions ?? []).map((p) => (
              <Tag key={p}>{permissionLabel(p)}</Tag>
            ))}
          </Space>
        ),
    },
    {
      title: t`最后使用`,
      dataIndex: "last_used_at",
      width: 160,
      render: (_, row) =>
        row.last_used_at ? dayjs(row.last_used_at).format("YYYY-MM-DD HH:mm") : "-",
    },
    {
      title: t`过期`,
      dataIndex: "expires_at",
      width: 160,
      render: (_, row) =>
        row.expires_at ? dayjs(row.expires_at).format("YYYY-MM-DD HH:mm") : t`永不`,
    },
    {
      title: t`状态`,
      width: 100,
      render: (_, row) => <TokenStatusTag token={row} />,
    },
    {
      title: t`操作`,
      width: 90,
      render: (_, row) =>
        tokenStatus(row) === "revoked" ? null : (
          <Popconfirm
            title={t`撤销该令牌？`}
            description={t`撤销后使用该令牌的 CLI / CI 会立即失效`}
            onConfirm={() => handleRevoke(row.id)}
            okText={t`撤销`}
            cancelText={t`取消`}
          >
            <Button type="link" size="small" danger>
              <Trans>撤销</Trans>
            </Button>
          </Popconfirm>
        ),
    },
  ];

  return (
    <PageContainer title={t`令牌`} breadcrumbRender={false}>
      <ProTable<ApiToken>
        rowKey="id"
        search={false}
        options={false}
        loading={tokensQuery.isPending}
        dataSource={tokens}
        pagination={{ pageSize: 20, hideOnSinglePage: true }}
        locale={{ emptyText: <Empty description={t`还没有令牌`} /> }}
        toolBarRender={() =>
          canManage ? [<CreateTokenDrawer key="create" has={has} onFinish={handleCreate} />] : []
        }
        columns={columns}
      />

      <TokenRevealModal token={revealed} onClose={() => setRevealed(null)} />
    </PageContainer>
  );
}

const TOKEN_STATUS_LABELS: Record<TokenStatus, ReactNode> = {
  active: <Trans>活跃</Trans>,
  expired: <Trans>已过期</Trans>,
  revoked: <Trans>已撤销</Trans>,
};

function TokenStatusTag({ token }: { token: ApiToken }) {
  const status = tokenStatus(token);
  return <Tag color={tokenStatusColor(status)}>{TOKEN_STATUS_LABELS[status]}</Tag>;
}

function CreateTokenDrawer({
  has,
  onFinish,
}: {
  has: (p: PermissionName) => boolean;
  onFinish: (v: CreateTokenValues) => Promise<boolean>;
}) {
  const { t } = useLingui();
  // 可授予的权限 = 自己拥有的子集(后端 validate_permissions 同样兜底 ⊆ creator)。
  const grantable = ALL_PERMISSIONS.filter((p) => has(p));

  return (
    <DrawerForm<CreateTokenValues>
      title={t`创建令牌`}
      trigger={
        <Button type="primary" icon={<PlusOutlined />}>
          <Trans>创建令牌</Trans>
        </Button>
      }
      drawerProps={{ destroyOnClose: true }}
      initialValues={{ kind: "api" }}
      onFinish={onFinish}
    >
      <ProFormRadio.Group
        name="kind"
        label={t`类型`}
        rules={[{ required: true }]}
        options={[
          { label: t`API（CI / 脚本，限定权限子集）`, value: "api" },
          { label: t`PAT（个人，继承本人全部权限）`, value: "pat" },
        ]}
      />
      <ProFormText
        name="name"
        label={t`名称`}
        tooltip={t`便于在列表里辨识，如 ci-github-actions`}
        rules={[{ required: true, message: t`请输入名称` }]}
      />
      <ProFormDependency name={["kind"]}>
        {({ kind }) =>
          kind === "api" ? (
            <ProFormSelect
              name="permissions"
              label={t`权限`}
              mode="multiple"
              tooltip={t`只能授予你自己拥有的权限`}
              options={grantable.map((p) => ({ label: permissionLabel(p), value: p }))}
              rules={[{ required: true, message: t`至少选择一个权限` }]}
            />
          ) : null
        }
      </ProFormDependency>
      <ProFormDatePicker
        name="expires_at"
        label={t`过期时间（可选）`}
        tooltip={t`留空则永不过期`}
        fieldProps={{ showTime: true, style: { width: "100%" } }}
      />
    </DrawerForm>
  );
}

function TokenRevealModal({
  token,
  onClose,
}: {
  token: CreateTokenResponse | null;
  onClose: () => void;
}) {
  const { t } = useLingui();
  return (
    <Modal
      title={t`令牌已创建`}
      open={token != null}
      onCancel={onClose}
      onOk={onClose}
      okText={t`我已保存`}
      cancelButtonProps={{ style: { display: "none" } }}
      destroyOnClose
    >
      <Alert
        type="warning"
        showIcon
        message={t`请立即复制保存。关闭后将无法再次查看完整令牌。`}
        style={{ marginBottom: 12 }}
      />
      <Typography.Paragraph
        copyable={{ text: token?.token }}
        code
        style={{ wordBreak: "break-all" }}
      >
        {token?.token}
      </Typography.Paragraph>
      <Typography.Paragraph type="secondary">
        <Trans>在 CI 中设为环境变量 SWARMHIVE_TOKEN 使用。</Trans>
      </Typography.Paragraph>
    </Modal>
  );
}
