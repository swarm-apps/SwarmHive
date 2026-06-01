import { GithubOutlined } from "@ant-design/icons";
import {
  PageContainer,
  ProForm,
  type ProFormInstance,
  ProFormText,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { Alert, App, Button, Card, List, Space, Tag, Typography } from "antd";
import dayjs from "dayjs";
import { useRef, useState } from "react";
import { isApiError } from "@/lib/api";
import { changePassword, patchDisplayName } from "@/lib/api/account";
import {
  identityLinksQueryOptions,
  oauthLinkStartUrl,
  publicProvidersQueryOptions,
  unlinkIdentity,
} from "@/lib/api/oauth";
import { meQueryOptions } from "@/lib/query/meQuery";
import { useResendVerify } from "@/lib/query/useResendVerify";

export const Route = createFileRoute("/_auth/profile")({
  component: ProfilePage,
});

type TabKey = "info" | "security" | "methods";

function ProfilePage() {
  const { t } = useLingui();
  const [tab, setTab] = useState<TabKey>("info");

  return (
    <PageContainer
      title={t`个人资料`}
      breadcrumbRender={false}
      tabActiveKey={tab}
      onTabChange={(key) => setTab(key as TabKey)}
      tabList={[
        { key: "info", tab: t`账户信息` },
        { key: "security", tab: t`安全` },
        { key: "methods", tab: t`登录方式` },
      ]}
    >
      {tab === "info" && <AccountInfoTab />}
      {tab === "security" && <SecurityTab />}
      {tab === "methods" && <LoginMethodsTab />}
    </PageContainer>
  );
}

// ─────────────────────────── 账户信息 ───────────────────────────

function AccountInfoTab() {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();
  const me = useQuery({ ...meQueryOptions(), retry: false });
  const resend = useResendVerify();

  const user = me.data?.user;
  const verified = user?.email_verified_at != null;

  return (
    <Card title={<Trans>账户信息</Trans>} loading={me.isPending}>
      {user && (
        <ProForm
          // 仅一个保存按钮，去掉默认重置按钮。
          submitter={{
            searchConfig: { submitText: t`保存` },
            resetButtonProps: false,
            submitButtonProps: { style: { marginInlineStart: 0 } },
          }}
          initialValues={{ email: user.email, display_name: user.display_name }}
          onFinish={async (values: { display_name: string }) => {
            try {
              await patchDisplayName(values.display_name);
              await queryClient.invalidateQueries({ queryKey: meQueryOptions().queryKey });
              notification.success({ message: t`已保存` });
              return true;
            } catch (error) {
              notification.error({
                message: t`保存失败`,
                description: isApiError(error) ? error.detail : String(error),
              });
              return false;
            }
          }}
        >
          <ProFormText
            name="email"
            label={t`邮箱`}
            disabled
            // 改邮箱需重新验证流程，暂不支持。
            tooltip={t`邮箱暂不支持自助修改`}
          />
          <ProFormText
            name="display_name"
            label={t`显示名称`}
            rules={[
              { required: true, message: t`请输入显示名称` },
              { max: 100, message: t`最多 100 个字符` },
            ]}
          />
        </ProForm>
      )}

      <div style={{ marginTop: 8 }}>
        <Space>
          <Typography.Text type="secondary">
            <Trans>邮箱验证状态</Trans>
          </Typography.Text>
          {verified ? (
            <Tag color="success">
              <Trans>已验证</Trans>
            </Tag>
          ) : (
            <Space>
              <Tag color="default">
                <Trans>未验证</Trans>
              </Tag>
              <Button
                size="small"
                type="link"
                loading={resend.isPending}
                onClick={() => resend.mutate()}
              >
                <Trans>重发验证邮件</Trans>
              </Button>
            </Space>
          )}
          {verified && user?.email_verified_at && (
            <Typography.Text type="secondary">
              {dayjs(user.email_verified_at).format("YYYY-MM-DD HH:mm")}
            </Typography.Text>
          )}
        </Space>
      </div>
    </Card>
  );
}

// ─────────────────────────── 安全（密码）───────────────────────────

function SecurityTab() {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();
  const me = useQuery({ ...meQueryOptions(), retry: false });
  // formRef 用于改密成功后清空表单——ProForm 的 onFinish 返回 true 不会自动
  // resetFields（那只对 ModalForm/DrawerForm 控制弹窗有意义），明文密码会残留。
  const formRef = useRef<ProFormInstance>(undefined);

  // 无密码（OAuth-only）→ 首次「设置密码」，不要求当前密码。
  const hasPassword = me.data?.has_password === true;

  return (
    <Card
      title={hasPassword ? <Trans>修改密码</Trans> : <Trans>设置密码</Trans>}
      loading={me.isPending}
    >
      {!hasPassword && (
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 16 }}
          message={<Trans>你还没有设置密码</Trans>}
          description={
            <Trans>
              当前账户通过第三方登录。设置密码后即可用邮箱 + 密码登录，并可解绑第三方账号。
            </Trans>
          }
        />
      )}
      <ProForm
        formRef={formRef}
        submitter={{
          searchConfig: { submitText: hasPassword ? t`更新密码` : t`设置密码` },
          resetButtonProps: false,
          submitButtonProps: { style: { marginInlineStart: 0 } },
        }}
        onFinish={async (values: { current_password?: string; new_password: string }) => {
          try {
            await changePassword(
              hasPassword ? values.current_password : undefined,
              values.new_password,
            );
            await queryClient.invalidateQueries({ queryKey: meQueryOptions().queryKey });
            // 改密成功后清空表单，不让明文密码残留在表单态 / DOM。
            formRef.current?.resetFields();
            notification.success({
              message: t`密码已更新`,
              description: t`你在其它设备上的登录已失效，需要重新登录。`,
            });
            return true;
          } catch (error) {
            notification.error({
              message: t`操作失败`,
              description: isApiError(error) ? error.detail : String(error),
            });
            return false;
          }
        }}
      >
        {hasPassword && (
          <ProFormText.Password
            name="current_password"
            label={t`当前密码`}
            rules={[{ required: true, message: t`请输入当前密码` }]}
          />
        )}
        <ProFormText.Password
          name="new_password"
          label={t`新密码`}
          tooltip={t`至少 12 个字符，且混合大小写 / 数字 / 符号中的至少 3 类`}
          rules={[
            { required: true, message: t`请输入新密码` },
            { min: 12, message: t`至少 12 个字符` },
          ]}
        />
        <ProFormText.Password
          name="confirm"
          label={t`确认新密码`}
          dependencies={["new_password"]}
          rules={[
            { required: true, message: t`请再次输入新密码` },
            ({ getFieldValue }) => ({
              validator(_, value) {
                if (!value || getFieldValue("new_password") === value) {
                  return Promise.resolve();
                }
                return Promise.reject(new Error(t`两次输入的密码不一致`));
              },
            }),
          ]}
        />
      </ProForm>
    </Card>
  );
}

// ─────────────────────────── 登录方式（OAuth）───────────────────────────

function LoginMethodsTab() {
  const { t } = useLingui();
  const { notification, modal } = App.useApp();
  const queryClient = useQueryClient();

  const linksQuery = useQuery({ ...identityLinksQueryOptions(), retry: false });
  const providersQuery = useQuery(publicProvidersQueryOptions());
  const links = linksQuery.data ?? [];
  const providers = providersQuery.data ?? [];

  const githubEnabled = providers.some((p) => p.kind === "github");
  const githubLinked = links.some((l) => l.provider === "github");

  function handleUnlink(provider: string) {
    modal.confirm({
      title: t`解绑该登录方式？`,
      content: t`解绑后将无法用它登录；若这是你唯一的登录方式，请先设置密码。`,
      okText: t`解绑`,
      okButtonProps: { danger: true },
      cancelText: t`取消`,
      onOk: async () => {
        try {
          await unlinkIdentity(provider);
          notification.success({ message: t`已解绑` });
          await queryClient.invalidateQueries({
            queryKey: identityLinksQueryOptions().queryKey,
          });
        } catch (error) {
          notification.error({
            message: t`解绑失败`,
            description: isApiError(error) ? error.detail : String(error),
          });
        }
      },
    });
  }

  return (
    <Card title={<Trans>已绑定的登录方式</Trans>} loading={linksQuery.isPending}>
      <List
        dataSource={links}
        locale={{ emptyText: <Trans>暂无绑定的外部登录方式</Trans> }}
        renderItem={(link) => (
          <List.Item
            actions={
              link.provider === "github"
                ? [
                    <Button
                      key="unlink"
                      type="link"
                      danger
                      onClick={() => handleUnlink(link.provider)}
                    >
                      <Trans>解绑</Trans>
                    </Button>,
                  ]
                : []
            }
          >
            <List.Item.Meta
              avatar={link.provider === "github" ? <GithubOutlined /> : undefined}
              title={
                <Space>
                  <Tag color="blue">{link.provider}</Tag>
                  <Typography.Text type="secondary">{link.subject}</Typography.Text>
                </Space>
              }
              description={new Date(link.created_at).toLocaleString()}
            />
          </List.Item>
        )}
      />

      {githubEnabled && !githubLinked && (
        <Button
          icon={<GithubOutlined />}
          style={{ marginTop: 16 }}
          onClick={() => window.location.assign(oauthLinkStartUrl("github"))}
        >
          <Trans>绑定 GitHub</Trans>
        </Button>
      )}
    </Card>
  );
}
