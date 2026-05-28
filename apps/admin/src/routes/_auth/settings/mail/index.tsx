import {
  CheckCircleOutlined,
  DeleteOutlined,
  EditOutlined,
  PlusOutlined,
  SendOutlined,
  ThunderboltOutlined,
} from "@ant-design/icons";
import {
  type ActionType,
  DrawerForm,
  ProFormSelect,
  ProFormText,
  ProTable,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { App, Button, Popconfirm, Space, Tag } from "antd";
import { useRef, useState } from "react";
import { fetchClient, isApiError } from "@/lib/api";
import {
  type CreateProviderReq,
  type MailProvider,
  mailStatusQueryOptions,
  PROVIDERS_PATH,
  type UpdateProviderReq,
} from "@/lib/api/mail";

export const Route = createFileRoute("/_auth/settings/mail/")({
  component: MailProvidersPage,
});

type FormValues = CreateProviderReq;

const ENCRYPTION_OPTIONS = [
  { value: "starttls", label: "STARTTLS" },
  { value: "tls", label: "TLS" },
  { value: "none", label: "None" },
];

function MailProvidersPage() {
  const { t } = useLingui();
  const { notification, modal } = App.useApp();
  const queryClient = useQueryClient();
  const tableRef = useRef<ActionType | null>(null);
  const [editing, setEditing] = useState<MailProvider | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);

  const refresh = () => {
    tableRef.current?.reload();
    // mailStatus drives the __root.tsx fallback banner; activate / delete
    // can flip transport between smtp and console.
    queryClient.invalidateQueries({ queryKey: mailStatusQueryOptions().queryKey });
  };

  const handleSubmit = async (values: FormValues) => {
    try {
      if (editing) {
        const body: UpdateProviderReq = { ...values };
        // Empty password input means "leave unchanged" — drop it so the server
        // doesn't try to encrypt an empty string.
        if (!body.password) {
          delete body.password;
        }
        const { error } = await fetchClient.PUT("/api/v1/mail/providers/{id}", {
          params: { path: { id: editing.id } },
          body,
        });
        if (error) {
          throw error;
        }
        notification.success({ message: t`Provider 已更新` });
      } else {
        const { error } = await fetchClient.POST(PROVIDERS_PATH, { body: values });
        if (error) {
          throw error;
        }
        notification.success({ message: t`Provider 已创建` });
      }
      setDrawerOpen(false);
      setEditing(null);
      refresh();
      return true;
    } catch (error) {
      const detail = isApiError(error) ? error.detail : String(error);
      notification.error({ message: t`保存失败`, description: detail });
      return false;
    }
  };

  const handleActivate = (row: MailProvider) => {
    modal.confirm({
      title: t`激活该 Provider？`,
      content: t`激活后会立即替换当前发件通道，并把其他 Provider 设为未激活。`,
      okText: t`激活`,
      onOk: async () => {
        try {
          const { error } = await fetchClient.POST("/api/v1/mail/providers/{id}/activate", {
            params: { path: { id: row.id } },
          });
          if (error) {
            throw error;
          }
          notification.success({ message: t`已激活`, description: row.name });
          refresh();
        } catch (error) {
          const detail = isApiError(error) ? error.detail : String(error);
          notification.error({ message: t`激活失败`, description: detail });
        }
      },
    });
  };

  const handleDelete = async (row: MailProvider) => {
    try {
      const { error } = await fetchClient.DELETE("/api/v1/mail/providers/{id}", {
        params: { path: { id: row.id } },
      });
      if (error) {
        throw error;
      }
      notification.success({ message: t`已删除`, description: row.name });
      refresh();
    } catch (error) {
      const detail = isApiError(error) ? error.detail : String(error);
      notification.error({ message: t`删除失败`, description: detail });
    }
  };

  const handleTest = async (row: MailProvider) => {
    try {
      const { data, error } = await fetchClient.POST("/api/v1/mail/providers/{id}/test", {
        params: { path: { id: row.id } },
      });
      if (error) {
        throw error;
      }
      notification.success({
        message: t`自检邮件已发送`,
        description: data?.to ? t`收件人：${data.to}` : undefined,
      });
    } catch (error) {
      const detail = isApiError(error) ? error.detail : String(error);
      notification.error({ message: t`自检失败`, description: detail });
    }
  };

  return (
    <>
      <ProTable<MailProvider>
        actionRef={tableRef}
        rowKey="id"
        search={false}
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
            <Trans>新建 Provider</Trans>
          </Button>,
        ]}
        request={async () => {
          const { data, error } = await fetchClient.GET(PROVIDERS_PATH);
          if (error) {
            throw error;
          }
          return { data: data ?? [], success: true, total: data?.length ?? 0 };
        }}
        columns={[
          { title: t`名称`, dataIndex: "name" },
          {
            title: t`服务器`,
            render: (_, row) => `${row.host}:${row.port}`,
          },
          {
            title: t`加密`,
            dataIndex: "encryption",
            render: (_, row) => row.encryption.toUpperCase(),
          },
          {
            title: t`发件人`,
            dataIndex: "from_email",
            render: (_, row) =>
              row.from_name ? `${row.from_name} <${row.from_email}>` : row.from_email,
          },
          {
            title: t`状态`,
            render: (_, row) =>
              row.active ? (
                <Tag icon={<CheckCircleOutlined />} color="success">
                  <Trans>激活</Trans>
                </Tag>
              ) : (
                <Tag>
                  <Trans>未激活</Trans>
                </Tag>
              ),
          },
          {
            title: t`操作`,
            valueType: "option",
            render: (_, row) => (
              <Space>
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
                <Button
                  size="small"
                  icon={<SendOutlined />}
                  onClick={() => handleTest(row)}
                  disabled={!row.active}
                >
                  <Trans>发自检</Trans>
                </Button>
                {!row.active && (
                  <Button
                    size="small"
                    icon={<ThunderboltOutlined />}
                    onClick={() => handleActivate(row)}
                  >
                    <Trans>激活</Trans>
                  </Button>
                )}
                <Popconfirm
                  title={t`删除该 Provider？`}
                  description={t`该操作不可撤销。`}
                  onConfirm={() => handleDelete(row)}
                >
                  <Button size="small" danger icon={<DeleteOutlined />}>
                    <Trans>删除</Trans>
                  </Button>
                </Popconfirm>
              </Space>
            ),
          },
        ]}
      />

      {/* key 随 editing 变化强制 remount：DrawerForm 的 initialValues 只在
          首次挂载生效，不 remount 会残留上一次打开时的表单值。 */}
      <DrawerForm<FormValues>
        key={editing?.id ?? "new"}
        title={editing ? t`编辑 Provider` : t`新建 Provider`}
        open={drawerOpen}
        onOpenChange={(open) => {
          setDrawerOpen(open);
          if (!open) {
            setEditing(null);
          }
        }}
        initialValues={
          editing
            ? {
                name: editing.name,
                host: editing.host,
                port: editing.port,
                username: editing.username ?? undefined,
                encryption: editing.encryption,
                from_email: editing.from_email,
                from_name: editing.from_name ?? undefined,
                reply_to: editing.reply_to ?? undefined,
              }
            : { encryption: "starttls", port: 587 }
        }
        onFinish={handleSubmit}
      >
        <ProFormText
          name="name"
          label={t`显示名称`}
          rules={[{ required: true, message: t`必填` }]}
        />
        <ProFormText
          name="host"
          label={t`SMTP 主机`}
          rules={[{ required: true, message: t`必填` }]}
        />
        <ProFormText
          name="port"
          label={t`端口`}
          fieldProps={{ type: "number" }}
          rules={[{ required: true, message: t`必填` }]}
          transform={(value) => ({ port: Number(value) })}
        />
        <ProFormSelect
          name="encryption"
          label={t`加密方式`}
          options={ENCRYPTION_OPTIONS}
          rules={[{ required: true, message: t`必填` }]}
        />
        <ProFormText name="username" label={t`用户名（可选）`} />
        <ProFormText.Password
          name="password"
          label={editing ? t`密码（留空则不修改）` : t`密码（可选，匿名 SMTP 不填）`}
          fieldProps={{ autoComplete: "new-password" }}
        />
        <ProFormText
          name="from_email"
          label={t`发件地址`}
          rules={[
            { required: true, message: t`必填` },
            { type: "email", message: t`邮箱格式不正确` },
          ]}
        />
        <ProFormText name="from_name" label={t`发件人显示名称（可选）`} />
        <ProFormText name="reply_to" label={t`Reply-To（可选）`} />
      </DrawerForm>
    </>
  );
}
