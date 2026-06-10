import {
  type ActionType,
  ModalForm,
  PageContainer,
  ProFormTextArea,
  ProTable,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { createFileRoute } from "@tanstack/react-router";
import { App, Button, Space, Tag } from "antd";
import dayjs from "dayjs";
import { useRef, useState } from "react";
import { fetchClient, isApiError } from "@/lib/api";
import { postApproveUser, postRejectUser, type UserListItem } from "@/lib/api/account";
import { RoleSelect } from "./-shared";

export const Route = createFileRoute("/_auth/users/approvals")({
  component: ApprovalsPage,
});

function ApprovalsPage() {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const actionRef = useRef<ActionType>(null);
  /** 正在 批准 / 拒绝 的行(null = modal 关闭)。 */
  const [approving, setApproving] = useState<UserListItem | null>(null);
  const [rejecting, setRejecting] = useState<UserListItem | null>(null);

  // 表格是 ProTable request 驱动(非 useQuery),变更后 reload 即可;
  // 成员列表页同为 request 驱动,路由进入时自会重新拉取。
  const afterMutation = () => actionRef.current?.reload();

  async function handleApprove(values: { role_id?: string }): Promise<boolean> {
    if (!approving) return false;
    try {
      await postApproveUser(approving.id, values.role_id);
      notification.success({ message: t`已批准 ${approving.email}` });
      setApproving(null);
      afterMutation();
      return true;
    } catch (error) {
      notification.error({
        message: t`批准失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
      return false;
    }
  }

  async function handleReject(values: { reason?: string }): Promise<boolean> {
    if (!rejecting) return false;
    try {
      await postRejectUser(rejecting.id, values.reason?.trim() || undefined);
      notification.success({ message: t`已拒绝并删除该注册` });
      setRejecting(null);
      afterMutation();
      return true;
    } catch (error) {
      notification.error({
        message: t`拒绝失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
      return false;
    }
  }

  return (
    <PageContainer
      title={t`注册审批`}
      subTitle={t`自助注册的账号在管理员批准前不能使用任何功能`}
      breadcrumbRender={false}
    >
      <ProTable<UserListItem>
        rowKey="id"
        actionRef={actionRef}
        search={false}
        options={false}
        pagination={{ pageSize: 20 }}
        locale={{
          emptyText: <Trans>没有待审批的注册。陌生人自助注册后会出现在这里。</Trans>,
        }}
        // server 端分页:GET /users/pending-approval?page=&per_page=
        request={async (params) => {
          const { data, error } = await fetchClient.GET("/api/v1/users/pending-approval", {
            params: {
              query: {
                page: params.current ?? 1,
                per_page: params.pageSize ?? 20,
              },
            },
          });
          if (error) throw error;
          return {
            data: data?.items ?? [],
            success: true,
            total: data?.total ?? 0,
          };
        }}
        columns={[
          { title: t`邮箱`, dataIndex: "email" },
          { title: t`显示名称`, dataIndex: "display_name" },
          {
            title: t`注册时绑定角色`,
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
            title: t`注册时间`,
            dataIndex: "created_at",
            width: 180,
            render: (_, row) => dayjs(row.created_at).format("YYYY-MM-DD HH:mm"),
          },
          {
            title: t`操作`,
            width: 160,
            render: (_, row) => (
              <Space size="small">
                <Button type="link" size="small" onClick={() => setApproving(row)}>
                  <Trans>批准</Trans>
                </Button>
                <Button type="link" size="small" danger onClick={() => setRejecting(row)}>
                  <Trans>拒绝</Trans>
                </Button>
              </Space>
            ),
          },
        ]}
      />

      <ModalForm<{ role_id?: string }>
        key={approving?.id ?? "approve"}
        title={t`批准注册：${approving?.email ?? ""}`}
        open={approving !== null}
        onOpenChange={(o) => {
          if (!o) setApproving(null);
        }}
        modalProps={{ destroyOnClose: true, width: 420 }}
        // 注册时已按 policy 默认角色绑定;此处预填该角色,Owner 可覆盖。
        initialValues={{ role_id: approving?.roles[0]?.id }}
        onFinish={handleApprove}
      >
        <RoleSelect />
      </ModalForm>

      <ModalForm<{ reason?: string }>
        key={rejecting?.id ?? "reject"}
        title={t`拒绝注册：${rejecting?.email ?? ""}`}
        open={rejecting !== null}
        onOpenChange={(o) => {
          if (!o) setRejecting(null);
        }}
        modalProps={{ destroyOnClose: true, width: 420 }}
        submitter={{ submitButtonProps: { danger: true } }}
        onFinish={handleReject}
      >
        <ProFormTextArea
          name="reason"
          label={t`拒绝原因（可选，仅写入审计日志）`}
          placeholder={t`例如：非组织成员`}
        />
      </ModalForm>
    </PageContainer>
  );
}
