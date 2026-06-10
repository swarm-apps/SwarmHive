import { PlusOutlined } from "@ant-design/icons";
import {
  type ActionType,
  DrawerForm,
  ModalForm,
  PageContainer,
  ProFormText,
  ProTable,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { App, Button, Popconfirm, Space, Tag } from "antd";
import dayjs from "dayjs";
import { useRef, useState } from "react";
import { fetchClient, isApiError } from "@/lib/api";
import {
  ERR_CANNOT_INVITE_OWNER,
  ERR_EMAIL_ALREADY_TAKEN,
  type InviteReq,
  postDisableUser,
  postEnableUser,
  postInvite,
  postResendInvite,
  putUserRole,
  USERS_PATH,
  type UserListItem,
} from "@/lib/api/account";
import { meQueryOptions } from "@/lib/query/meQuery";
import { RoleSelect } from "./-shared";

export const Route = createFileRoute("/_auth/users/list")({
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
  const actionRef = useRef<ActionType>(null);
  const me = useQuery(meQueryOptions()).data;
  /** 正在「更改角色」的行(null = modal 关闭)。 */
  const [changingRole, setChangingRole] = useState<UserListItem | null>(null);

  // 表格是 ProTable request 驱动(非 useQuery),变更后 reload 即可。
  const reloadUsers = () => actionRef.current?.reload();

  async function handleInvite(values: InviteFormValues): Promise<boolean> {
    try {
      const body: InviteReq = {
        email: values.email.trim(),
        role_id: values.role_id,
        display_name: values.display_name?.trim() || undefined,
      };
      await postInvite(body);
      notification.success({ message: t`邀请已发送` });
      reloadUsers();
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
      reloadUsers();
    } catch {
      notification.error({ message: t`重发失败，请稍后重试` });
    }
  }

  async function handleChangeRole(values: { role_id?: string }): Promise<boolean> {
    if (!changingRole || !values.role_id) return false;
    try {
      await putUserRole(changingRole.id, values.role_id);
      notification.success({ message: t`已更改 ${changingRole.email} 的角色` });
      setChangingRole(null);
      reloadUsers();
      return true;
    } catch (error) {
      notification.error({
        message: t`更改角色失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
      return false;
    }
  }

  async function handleToggleStatus(row: UserListItem) {
    try {
      if (row.status === "active") {
        await postDisableUser(row.id);
        notification.success({ message: t`已禁用 ${row.email}（其全部会话已失效）` });
      } else {
        await postEnableUser(row.id);
        notification.success({ message: t`已启用 ${row.email}` });
      }
      reloadUsers();
    } catch (error) {
      notification.error({
        message: t`操作失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
    }
  }

  return (
    <PageContainer title={t`成员`} breadcrumbRender={false}>
      <ProTable<UserListItem>
        rowKey="id"
        actionRef={actionRef}
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
            filters: [
              { text: t`已激活`, value: "active" },
              { text: t`待接受`, value: "provisioned" },
              { text: t`待审批`, value: "pending_approval" },
              { text: t`已禁用`, value: "disabled" },
            ],
            onFilter: (value, row) => row.status === value,
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
            render: (_, row) => {
              if (row.status === "pending_approval") {
                // 批准 / 拒绝集中在专门的注册审批页,这里只给入口。
                return (
                  <Link to="/users/approvals">
                    <Button type="link" size="small">
                      <Trans>去审批</Trans>
                    </Button>
                  </Link>
                );
              }
              // owner 行与自己一律不显示管理操作(server 端同样拒绝)。
              const isOwnerRow = row.roles.some((r) => r.name === "owner");
              const isSelf = row.id === me?.user.id;
              if (isOwnerRow || isSelf) {
                return null;
              }
              return (
                <Space size="small">
                  {row.status === "provisioned" && (
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
                  )}
                  <Button type="link" size="small" onClick={() => setChangingRole(row)}>
                    <Trans>更改角色</Trans>
                  </Button>
                  {row.status === "active" && (
                    <Popconfirm
                      title={t`禁用该成员？其全部会话将立即失效。`}
                      onConfirm={() => handleToggleStatus(row)}
                      okText={t`禁用`}
                      okButtonProps={{ danger: true }}
                      cancelText={t`取消`}
                    >
                      <Button type="link" size="small" danger>
                        <Trans>禁用</Trans>
                      </Button>
                    </Popconfirm>
                  )}
                  {row.status === "disabled" && (
                    <Button type="link" size="small" onClick={() => handleToggleStatus(row)}>
                      <Trans>启用</Trans>
                    </Button>
                  )}
                </Space>
              );
            },
          },
        ]}
      />

      <ModalForm<{ role_id?: string }>
        key={changingRole?.id ?? "change-role"}
        title={t`更改角色：${changingRole?.email ?? ""}`}
        open={changingRole !== null}
        onOpenChange={(o) => {
          if (!o) setChangingRole(null);
        }}
        modalProps={{ destroyOnClose: true, width: 420 }}
        initialValues={{ role_id: changingRole?.roles[0]?.id }}
        onFinish={handleChangeRole}
      >
        <RoleSelect />
      </ModalForm>
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
    case "provisioned":
      return (
        <Tag color="warning">
          <Trans>待接受</Trans>
        </Tag>
      );
    case "pending_approval":
      return (
        <Tag color="processing">
          <Trans>待审批</Trans>
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
      <RoleSelect label={t`角色`} />
      <ProFormText
        name="display_name"
        label={t`显示名称（可选）`}
        rules={[{ max: 64, message: t`最长 64 个字符` }]}
      />
    </DrawerForm>
  );
}
