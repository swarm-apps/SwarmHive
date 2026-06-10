import { PlusOutlined } from "@ant-design/icons";
import {
  DrawerForm,
  PageContainer,
  type ProColumns,
  ProFormSelect,
  ProFormSwitch,
  ProFormText,
  ProTable,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { Alert, App, Button, Space, Tag } from "antd";
import { useState } from "react";
import { isApiError } from "@/lib/api";
import {
  createProvider,
  deleteProvider,
  type OAuthProviderView,
  providersQueryOptions,
  testProvider,
  updateProvider,
} from "@/lib/api/oauth";
import { usePermissions } from "@/lib/query/usePermissions";

export const Route = createFileRoute("/_auth/settings/authentication")({
  component: AuthenticationPage,
});

const GITHUB_DEFAULTS = {
  authorize_url: "https://github.com/login/oauth/authorize",
  token_url: "https://github.com/login/oauth/access_token",
  userinfo_url: "https://api.github.com/user",
  scopes: ["read:user", "user:email"],
};

interface FormValues {
  name: string;
  client_id: string;
  client_secret?: string;
  scopes: string[];
  authorize_url: string;
  token_url: string;
  userinfo_url: string;
  enabled: boolean;
}

function AuthenticationPage() {
  const { t } = useLingui();
  const { notification, modal } = App.useApp();
  const queryClient = useQueryClient();
  const { has } = usePermissions();

  const providersQuery = useQuery(providersQueryOptions());
  const providers = providersQuery.data ?? [];
  const canManage = has("auth:manage");

  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editing, setEditing] = useState<OAuthProviderView | null>(null);

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: providersQueryOptions().queryKey });

  async function handleSubmit(values: FormValues): Promise<boolean> {
    try {
      if (editing) {
        const body: Parameters<typeof updateProvider>[1] = {
          name: values.name,
          client_id: values.client_id,
          scopes: values.scopes,
          authorize_url: values.authorize_url,
          token_url: values.token_url,
          userinfo_url: values.userinfo_url,
          enabled: values.enabled,
        };
        // Blank = keep existing secret (mirrors storage / mail).
        if (values.client_secret) {
          body.client_secret = values.client_secret;
        }
        await updateProvider(editing.id, body);
        notification.success({ message: t`认证提供方已更新` });
      } else {
        await createProvider({
          kind: "github",
          name: values.name,
          client_id: values.client_id,
          client_secret: values.client_secret ?? "",
          scopes: values.scopes,
          authorize_url: values.authorize_url,
          token_url: values.token_url,
          userinfo_url: values.userinfo_url,
          enabled: values.enabled,
        });
        notification.success({ message: t`认证提供方已创建` });
      }
      setDrawerOpen(false);
      setEditing(null);
      await invalidate();
      return true;
    } catch (error) {
      notification.error({
        message: t`保存失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
      return false;
    }
  }

  async function handleTest(row: OAuthProviderView): Promise<void> {
    try {
      const result = await testProvider(row.id);
      if (result.ok) {
        notification.success({ message: t`检查通过`, description: result.detail });
      } else {
        notification.error({ message: t`检查失败`, description: result.detail });
      }
    } catch (error) {
      notification.error({
        message: t`测试失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
    }
  }

  async function handleToggle(row: OAuthProviderView): Promise<void> {
    try {
      await updateProvider(row.id, { enabled: !row.enabled });
      await invalidate();
    } catch (error) {
      notification.error({
        message: t`操作失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
    }
  }

  function handleDelete(row: OAuthProviderView) {
    modal.confirm({
      title: t`删除该认证提供方？`,
      content: t`删除后 /login 上对应的登录按钮会消失，已绑定该提供方的用户将无法用它登录。`,
      okText: t`删除`,
      okButtonProps: { danger: true },
      cancelText: t`取消`,
      onOk: async () => {
        try {
          await deleteProvider(row.id);
          notification.success({ message: t`已删除` });
          await invalidate();
        } catch (error) {
          notification.error({
            message: t`删除失败`,
            description: isApiError(error) ? error.detail : String(error),
          });
        }
      },
    });
  }

  const columns: ProColumns<OAuthProviderView>[] = [
    { title: t`名称`, dataIndex: "name" },
    {
      title: t`类型`,
      dataIndex: "kind",
      width: 100,
      render: (_, row) => <Tag color="blue">{row.kind}</Tag>,
    },
    { title: "Client ID", dataIndex: "client_id", ellipsis: true },
    {
      title: t`状态`,
      dataIndex: "enabled",
      width: 100,
      render: (_, row) =>
        row.enabled ? (
          <Tag color="green">
            <Trans>已启用</Trans>
          </Tag>
        ) : (
          <Tag>
            <Trans>已停用</Trans>
          </Tag>
        ),
    },
    {
      title: t`操作`,
      width: 240,
      render: (_, row) =>
        canManage ? (
          <Space size="small">
            <Button type="link" size="small" onClick={() => handleTest(row)}>
              <Trans>测试</Trans>
            </Button>
            <Button type="link" size="small" onClick={() => handleToggle(row)}>
              {row.enabled ? <Trans>停用</Trans> : <Trans>启用</Trans>}
            </Button>
            <Button
              type="link"
              size="small"
              onClick={() => {
                setEditing(row);
                setDrawerOpen(true);
              }}
            >
              <Trans>编辑</Trans>
            </Button>
            <Button type="link" size="small" danger onClick={() => handleDelete(row)}>
              <Trans>删除</Trans>
            </Button>
          </Space>
        ) : null,
    },
  ];

  return (
    <PageContainer title={t`认证`} breadcrumbRender={false}>
      <ProTable<OAuthProviderView>
        rowKey="id"
        search={false}
        options={false}
        loading={providersQuery.isPending}
        dataSource={providers}
        pagination={false}
        locale={{
          emptyText: (
            <Trans>还没有配置 OAuth 提供方。新建并启用后，/login 会出现对应登录按钮。</Trans>
          ),
        }}
        toolBarRender={() =>
          canManage
            ? [
                <Button
                  key="create"
                  type="primary"
                  icon={<PlusOutlined />}
                  onClick={() => {
                    setEditing(null);
                    setDrawerOpen(true);
                  }}
                >
                  <Trans>新建提供方</Trans>
                </Button>,
              ]
            : []
        }
        columns={columns}
      />

      <Alert
        type="info"
        showIcon
        style={{ marginTop: 16 }}
        title={
          <Trans>
            OAuth 默认仅用于已有成员登录 / 绑定。陌生人首次用 OAuth 登录的自助注册行为，由{" "}
            <Link to="/settings/registration">注册策略</Link> 页控制。
          </Trans>
        }
      />

      <ProviderDrawer
        editing={editing}
        open={drawerOpen}
        onOpenChange={(o) => {
          setDrawerOpen(o);
          if (!o) setEditing(null);
        }}
        onFinish={handleSubmit}
      />
    </PageContainer>
  );
}

function ProviderDrawer({
  editing,
  open,
  onOpenChange,
  onFinish,
}: {
  editing: OAuthProviderView | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onFinish: (v: FormValues) => Promise<boolean>;
}) {
  const { t } = useLingui();
  return (
    <DrawerForm<FormValues>
      key={editing?.id ?? "new"}
      title={editing ? t`编辑提供方` : t`新建提供方`}
      open={open}
      onOpenChange={onOpenChange}
      drawerProps={{ destroyOnClose: true }}
      initialValues={
        editing
          ? {
              name: editing.name,
              client_id: editing.client_id,
              scopes: editing.scopes,
              authorize_url: editing.authorize_url,
              token_url: editing.token_url,
              userinfo_url: editing.userinfo_url,
              enabled: editing.enabled,
            }
          : {
              name: "GitHub",
              scopes: GITHUB_DEFAULTS.scopes,
              authorize_url: GITHUB_DEFAULTS.authorize_url,
              token_url: GITHUB_DEFAULTS.token_url,
              userinfo_url: GITHUB_DEFAULTS.userinfo_url,
              enabled: false,
            }
      }
      onFinish={onFinish}
    >
      <ProFormSelect
        name="kind"
        label={t`类型`}
        initialValue="github"
        disabled
        options={[{ label: "GitHub", value: "github" }]}
        tooltip={t`MVP 仅支持 GitHub`}
      />
      <ProFormText
        name="name"
        label={t`显示名称`}
        rules={[{ required: true, message: t`请输入名称` }]}
      />
      <ProFormText
        name="client_id"
        label="Client ID"
        rules={[{ required: true, message: t`请输入 Client ID` }]}
      />
      <ProFormText.Password
        name="client_secret"
        label="Client Secret"
        placeholder={editing ? t`留空表示不修改` : undefined}
        rules={editing ? [] : [{ required: true, message: t`请输入 Client Secret` }]}
      />
      <ProFormSelect
        name="scopes"
        label="Scopes"
        mode="tags"
        fieldProps={{ tokenSeparators: [",", " "] }}
      />
      <ProFormSwitch name="enabled" label={t`启用（显示在 /login）`} />
      <ProFormText name="authorize_url" label="Authorize URL" />
      <ProFormText name="token_url" label="Token URL" />
      <ProFormText name="userinfo_url" label="Userinfo URL" />
    </DrawerForm>
  );
}
