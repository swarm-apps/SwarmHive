import { PlusOutlined } from "@ant-design/icons";
import {
  DrawerForm,
  PageContainer,
  ProFormSelect,
  ProFormText,
  ProTable,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { App, Button, Popconfirm, Tag } from "antd";
import dayjs from "dayjs";
import { fetchClient, isApiError } from "@/lib/api";
import {
  ERR_CANNOT_INVITE_OWNER,
  ERR_EMAIL_ALREADY_TAKEN,
  type InviteReq,
  postInvite,
  postResendInvite,
  type Role,
  rolesQueryOptions,
  USERS_PATH,
  type UserListItem,
  usersQueryOptions,
} from "@/lib/api/account";

export const Route = createFileRoute("/_auth/users")({
  component: UsersPage,
});

interface InviteFormValues {
  email: string;
  confirm_email: string;
  role_id: string;
  display_name?: string;
}

function UsersPage() {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();

  const invalidateUsers = () =>
    queryClient.invalidateQueries({ queryKey: usersQueryOptions().queryKey });

  async function handleInvite(values: InviteFormValues): Promise<boolean> {
    try {
      const body: InviteReq = {
        email: values.email.trim(),
        role_id: values.role_id,
        display_name: values.display_name?.trim() || undefined,
      };
      await postInvite(body);
      notification.success({ message: t`邀请已发送` });
      await invalidateUsers();
      return true;
    } catch (error) {
      if (isApiError(error)) {
        switch (error.type) {
          case ERR_EMAIL_ALREADY_TAKEN:
            notification.error({ message: t`该邮箱已被占用` });
            return false;
          case ERR_CANNOT_INVITE_OWNER:
            notification.error({ message: t`不能邀请 Owner 角色` });
            return false;
        }
      }
      notification.error({ message: t`邀请失败，请稍后重试` });
      return false;
    }
  }

  async function handleResend(id: string) {
    try {
      await postResendInvite(id);
      notification.success({ message: t`已重新发送邀请邮件` });
      await invalidateUsers();
    } catch {
      notification.error({ message: t`重发失败，请稍后重试` });
    }
  }

  return (
    <PageContainer title={t`成员`} breadcrumbRender={false}>
      <ProTable<UserListItem>
        rowKey="id"
        search={false}
        pagination={{ pageSize: 50 }}
        toolBarRender={() => [<InviteDrawer key="invite" onFinish={handleInvite} />]}
        request={async () => {
          const { data, error } = await fetchClient.GET(USERS_PATH);
          if (error) throw error;
          return { data: data ?? [], success: true, total: data?.length ?? 0 };
        }}
        columns={[
          { title: t`邮箱`, dataIndex: "email" },
          { title: t`显示名称`, dataIndex: "display_name" },
          {
            title: t`角色`,
            dataIndex: "roles",
            render: (_, row) =>
              row.roles.length > 0 ? (
                row.roles.map((r) => (
                  <Tag key={r.id} color="blue">
                    {r.name}
                  </Tag>
                ))
              ) : (
                <span style={{ color: "rgba(0,0,0,0.45)" }}>-</span>
              ),
          },
          {
            title: t`状态`,
            dataIndex: "status",
            width: 120,
            render: (_, row) => <StatusTag status={row.status} />,
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
            render: (_, row) =>
              row.status === "invited" ? (
                <Popconfirm
                  title={t`重新发送邀请邮件？`}
                  onConfirm={() => handleResend(row.id)}
                  okText={t`重发`}
                  cancelText={t`取消`}
                >
                  <Button type="link" size="small">
                    <Trans>重发邀请</Trans>
                  </Button>
                </Popconfirm>
              ) : null,
          },
        ]}
      />
    </PageContainer>
  );
}

function StatusTag({ status }: { status: UserListItem["status"] }) {
  switch (status) {
    case "active":
      return (
        <Tag color="success">
          <Trans>已激活</Trans>
        </Tag>
      );
    case "invited":
      return (
        <Tag color="warning">
          <Trans>待接受</Trans>
        </Tag>
      );
    case "disabled":
      return (
        <Tag color="default">
          <Trans>已禁用</Trans>
        </Tag>
      );
    default:
      return <Tag>{status}</Tag>;
  }
}

function InviteDrawer({ onFinish }: { onFinish: (v: InviteFormValues) => Promise<boolean> }) {
  const { t } = useLingui();
  const queryClient = useQueryClient();

  return (
    <DrawerForm<InviteFormValues>
      title={t`邀请成员`}
      trigger={
        <Button type="primary" icon={<PlusOutlined />}>
          <Trans>邀请成员</Trans>
        </Button>
      }
      drawerProps={{ destroyOnClose: true }}
      onFinish={onFinish}
    >
      <ProFormText
        name="email"
        label={t`邮箱`}
        rules={[
          { required: true, message: t`请输入邮箱` },
          { type: "email", message: t`邮箱格式不正确` },
        ]}
      />
      <ProFormText
        name="confirm_email"
        label={t`确认邮箱`}
        dependencies={["email"]}
        rules={[
          { required: true, message: t`请再次输入邮箱` },
          ({ getFieldValue }) => ({
            validator: (_, value) =>
              !value || value === getFieldValue("email")
                ? Promise.resolve()
                : Promise.reject(new Error(t`两次输入的邮箱不一致`)),
          }),
        ]}
      />
      <ProFormSelect
        name="role_id"
        label={t`角色`}
        rules={[{ required: true, message: t`请选择角色` }]}
        request={async () => {
          const roles = await queryClient.ensureQueryData(rolesQueryOptions());
          return (roles as Role[])
            .filter((r) => r.name.toLowerCase() !== "owner")
            .map((r) => ({ label: r.name, value: r.id }));
        }}
      />
      <ProFormText
        name="display_name"
        label={t`显示名称（可选）`}
        rules={[{ max: 64, message: t`最长 64 个字符` }]}
      />
    </DrawerForm>
  );
}
