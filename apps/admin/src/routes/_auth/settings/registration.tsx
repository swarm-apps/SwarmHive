import {
  PageContainer,
  ProForm,
  ProFormDependency,
  ProFormSelect,
  ProFormSwitch,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { Alert, App, Card, Empty } from "antd";
import { isApiError } from "@/lib/api";
import { rolesQueryOptions } from "@/lib/api/account";
import { mailStatusQueryOptions } from "@/lib/api/mail";
import {
  type RegistrationPolicy,
  registrationPolicyQueryOptions,
  updateRegistrationPolicy,
} from "@/lib/api/registration";
import { usePermissions } from "@/lib/query/usePermissions";

export const Route = createFileRoute("/_auth/settings/registration")({
  component: RegistrationPage,
});

function RegistrationPage() {
  const { t } = useLingui();
  const queryClient = useQueryClient();
  const { has } = usePermissions();
  const canManage = has("auth:manage");

  // policy 端点要求 auth:manage,无权限时不发请求。
  const policyQuery = useQuery({ ...registrationPolicyQueryOptions(), enabled: canManage });
  const mailStatusQuery = useQuery(mailStatusQueryOptions());
  const policy = policyQuery.data;
  const showMailWarning =
    mailStatusQuery.data?.fallback_mode === true &&
    policy?.allow_self_register_email === true &&
    policy.require_email_verify;

  return (
    <PageContainer title={t`注册策略`} breadcrumbRender={false}>
      {showMailWarning && (
        <Alert
          type="warning"
          showIcon
          style={{ marginBottom: 16 }}
          title={<Trans>邮箱验证已启用,但 Mail 尚未配置</Trans>}
          description={
            <Trans>
              自助注册要求验证邮箱,而当前没有活跃的 SMTP provider——注册流程会卡在收不到验证邮件。
              请先到 Settings › Mail 配置并激活一个 provider。
            </Trans>
          }
        />
      )}
      <Card>
        {canManage && policy ? (
          <RegistrationPolicyForm
            policy={policy}
            onSaved={() =>
              queryClient.invalidateQueries({
                queryKey: registrationPolicyQueryOptions().queryKey,
              })
            }
          />
        ) : (
          <Empty
            description={
              canManage ? (
                <Trans>加载中…</Trans>
              ) : (
                <Trans>需要 auth:manage 权限才能查看注册策略。</Trans>
              )
            }
          />
        )}
      </Card>
      <Alert
        type="info"
        showIcon
        style={{ marginTop: 16 }}
        title={
          <Trans>
            自助注册默认全关:陌生人只能经管理员邀请加入。OAuth 登录按钮本身由 Settings › 认证
            的提供方配置控制,这里只决定「陌生人能否自助建号」。
          </Trans>
        }
      />
    </PageContainer>
  );
}

interface PolicyFormValues {
  allow_self_register_email: boolean;
  allow_self_register_oauth: boolean;
  require_email_verify: boolean;
  self_register_default_role_id: string;
  self_register_require_approval: boolean;
  allowed_email_domains: string[];
}

function RegistrationPolicyForm({
  policy,
  onSaved,
}: {
  policy: RegistrationPolicy;
  onSaved: () => void;
}) {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const rolesQuery = useQuery(rolesQueryOptions());
  const roles = rolesQuery.data ?? [];

  async function handleSave(values: PolicyFormValues): Promise<void> {
    try {
      await updateRegistrationPolicy({
        ...values,
        // 服务端按 lowercase 精确匹配,提交前统一归一化。
        allowed_email_domains: values.allowed_email_domains.map((d) => d.trim().toLowerCase()),
      });
      notification.success({ message: t`注册策略已保存` });
      onSaved();
    } catch (error) {
      notification.error({
        message: t`保存失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
      throw error;
    }
  }

  return (
    <ProForm<PolicyFormValues>
      key={policy.updated_at}
      initialValues={{
        allow_self_register_email: policy.allow_self_register_email,
        allow_self_register_oauth: policy.allow_self_register_oauth,
        require_email_verify: policy.require_email_verify,
        self_register_default_role_id: policy.self_register_default_role_id,
        self_register_require_approval: policy.self_register_require_approval,
        allowed_email_domains: policy.allowed_email_domains,
      }}
      onFinish={handleSave}
      submitter={{
        searchConfig: { submitText: t`保存` },
        resetButtonProps: false,
      }}
    >
      <ProFormSwitch
        name="allow_self_register_oauth"
        label={t`允许 OAuth 自助注册`}
        tooltip={t`开启后，陌生人首次用 GitHub 登录会自动创建账号（受下方域白名单约束）`}
      />
      <ProFormSwitch
        name="allow_self_register_email"
        label={t`允许邮箱自助注册`}
        tooltip={t`开启后 /register 页可用，/login 显示注册入口`}
      />
      <ProFormDependency name={["allow_self_register_email"]}>
        {({ allow_self_register_email }) => (
          <ProFormSwitch
            name="require_email_verify"
            label={t`要求验证邮箱`}
            disabled={!allow_self_register_email}
            tooltip={t`仅作用于邮箱自助注册；OAuth 的 verified email 直接视为已验证`}
          />
        )}
      </ProFormDependency>
      <ProFormSwitch
        name="self_register_require_approval"
        label={t`需管理员审批`}
        tooltip={t`开启后，自助注册的账号进入待审批状态，由 Users 页 Approve / Reject`}
      />
      <ProFormSelect
        name="self_register_default_role_id"
        label={t`默认角色`}
        rules={[{ required: true, message: t`请选择默认角色` }]}
        fieldProps={{ loading: rolesQuery.isPending }}
        options={roles.map((r) => ({
          label: r.name,
          value: r.id,
          // 服务端同样拒绝 owner;前端先禁掉避免无效提交。
          disabled: r.name === "owner",
        }))}
        tooltip={t`自助注册者自动绑定的角色（不可选 owner）`}
      />
      <ProFormSelect
        name="allowed_email_domains"
        label={t`邮箱域白名单`}
        mode="tags"
        fieldProps={{ tokenSeparators: [",", " "] }}
        placeholder="example.com"
        tooltip={t`留空表示不限制；非空时仅这些域名的邮箱可自助注册（含 OAuth 路径）`}
      />
    </ProForm>
  );
}
