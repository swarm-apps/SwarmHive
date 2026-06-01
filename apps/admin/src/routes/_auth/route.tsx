import {
  AppstoreOutlined,
  BarChartOutlined,
  CloudOutlined,
  DashboardOutlined,
  KeyOutlined,
  LogoutOutlined,
  MailOutlined,
  RocketOutlined,
  SafetyOutlined,
  SettingOutlined,
  TeamOutlined,
  UserOutlined,
} from "@ant-design/icons";
import { ProLayout } from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useMutation, useQuery } from "@tanstack/react-query";
import { createFileRoute, Link, Outlet, redirect, useRouterState } from "@tanstack/react-router";
import { Alert, Button, Dropdown, Space, Spin } from "antd";
import { isApiError } from "@/lib/api";
import { postLogout } from "@/lib/api/account";
import { mailStatusQueryOptions } from "@/lib/api/mail";
import { meQueryOptions } from "@/lib/query/meQuery";
import { usePermissions } from "@/lib/query/usePermissions";
import { useResendVerify } from "@/lib/query/useResendVerify";
import { ColorModeToggle, useColorModeContext } from "@/lib/theme";

export const Route = createFileRoute("/_auth")({
  beforeLoad: async ({ context, location }) => {
    try {
      await context.queryClient.ensureQueryData(meQueryOptions());
    } catch (error) {
      if (isApiError(error) && error.status === 401) {
        throw redirect({
          to: "/login",
          search: { next: location.pathname },
          replace: true,
        });
      }
      throw error;
    }
  },
  component: AuthLayout,
});

function AuthLayout() {
  const { t } = useLingui();
  // 用 useRouterState 订阅 location 变化——useRouter().state 不是响应式的，
  // 导航时 AuthLayout 不会重渲染，会导致 ProLayout 菜单选中 / 面包屑卡在旧路由。
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const { resolved } = useColorModeContext();
  const me = useQuery({ ...meQueryOptions(), retry: false });
  const mailStatus = useQuery({ ...mailStatusQueryOptions(), retry: false });
  const { has } = usePermissions();

  // 设置区父菜单：持任一「已上线模块」的 manage 权限即可见（mail / auth / storage）。
  const canManageSettings = has("mail:manage") || has("auth:manage") || has("storage:manage");
  const canManageUsers = has("user:manage");
  // Only nag operators in non-dev builds — local Vite dev defaults to the
  // mailpit provider so the banner would be noise.
  const showFallbackBanner = !import.meta.env.DEV && mailStatus.data?.fallback_mode === true;
  // Email-verification nag: shown whenever the current user's email is
  // unverified. Persistent (not closable) — verifying or configuring SMTP is
  // the only way to dismiss it.
  const showVerifyBanner = me.data != null && me.data.user.email_verified_at == null;
  const mailFallback = mailStatus.data?.fallback_mode === true;

  const usersRoute = canManageUsers
    ? [{ path: "/users", name: t`成员`, icon: <TeamOutlined /> }]
    : [];

  // 「设置」是组织 / 部署级配置，仅持任一 *:manage 权限者可见。个人账户（信息 /
  // 安全 / 登录方式）统一在头像下拉的 /profile，不再挂这里。
  const settingsRoute = canManageSettings
    ? [
        {
          path: "/settings",
          name: t`设置`,
          icon: <SettingOutlined />,
          routes: [
            { path: "/settings/mail", name: t`邮件`, icon: <MailOutlined /> },
            { path: "/settings/authentication", name: t`认证`, icon: <SafetyOutlined /> },
            { path: "/settings/storage", name: t`存储`, icon: <CloudOutlined /> },
            {
              path: "/settings/telemetry",
              name: t`遥测`,
              icon: <BarChartOutlined />,
              disabled: true,
            },
          ],
        },
      ]
    : [];

  return (
    <ProLayout
      title="SwarmHive"
      logo={false}
      layout="mix"
      fixSiderbar
      fixedHeader
      navTheme={resolved === "dark" ? "realDark" : "light"}
      location={{ pathname }}
      menuItemRender={(item, dom) => {
        if (item.disabled) {
          return <span style={{ color: "rgba(0,0,0,0.25)", cursor: "not-allowed" }}>{dom}</span>;
        }
        return <Link to={item.path ?? "/"}>{dom}</Link>;
      }}
      subMenuItemRender={(_item, dom) => dom}
      route={{
        path: "/",
        routes: [
          { path: "/", name: t`仪表盘`, icon: <DashboardOutlined /> },
          { path: "/apps", name: t`应用`, icon: <AppstoreOutlined /> },
          { path: "/releases", name: t`版本`, icon: <RocketOutlined /> },
          { path: "/tokens", name: t`令牌`, icon: <KeyOutlined /> },
          ...usersRoute,
          ...settingsRoute,
        ],
      }}
      actionsRender={() => [<ColorModeToggle key="color-mode" />]}
      avatarProps={{
        render: (_props, dom) => <UserAvatar fallback={dom} />,
      }}
    >
      {showVerifyBanner && (
        <VerifyBanner email={me.data?.user.email ?? ""} mailFallback={mailFallback} />
      )}
      {showFallbackBanner && (
        <Alert
          banner
          closable
          type="warning"
          showIcon
          message={<Trans>邮件未配置，部分功能（邀请 / 密码重置）将不可用。</Trans>}
          action={
            <Link to="/settings/mail">
              <Trans>前往配置</Trans>
            </Link>
          }
          style={{ marginBottom: 16 }}
        />
      )}
      <Outlet />
    </ProLayout>
  );
}

function VerifyBanner({ email, mailFallback }: { email: string; mailFallback: boolean }) {
  const resend = useResendVerify();

  // When mail is in fallback mode the resend would 422 anyway — point the
  // operator at the SMTP wizard instead of offering a dead resend button.
  if (mailFallback) {
    return (
      <Alert
        banner
        type="warning"
        showIcon
        message={<Trans>邮箱未验证，且邮件服务未配置。请先配置 SMTP 后再验证。</Trans>}
        action={
          <Link to="/settings/mail">
            <Trans>配置 SMTP</Trans>
          </Link>
        }
        style={{ marginBottom: 16 }}
      />
    );
  }

  return (
    <Alert
      banner
      type="warning"
      showIcon
      message={
        <Trans>
          你的邮箱 <strong>{email}</strong> 尚未验证。
        </Trans>
      }
      action={
        <Button size="small" type="link" loading={resend.isPending} onClick={() => resend.mutate()}>
          <Trans>重发验证邮件</Trans>
        </Button>
      }
      style={{ marginBottom: 16 }}
    />
  );
}

function UserAvatar({ fallback }: { fallback: React.ReactNode }) {
  const me = useQuery({ ...meQueryOptions(), retry: false });
  // logout 后整页跳登录：window.location.assign 会触发 full reload，顺带清空
  // 所有内存态（react-query 缓存、Context 等），是 security-sensitive 登出最稳的
  // 做法。用 onSettled 而非 onSuccess——即便请求失败也把用户带去登录页（best-effort，
  // 与 CLI logout「尽力撤销 + 清本地」同语义）；成功时 server 已 session.delete()，
  // 非 401 错误由 client middleware 自动弹 notification。
  const logout = useMutation({
    mutationFn: postLogout,
    onSettled: () => {
      window.location.assign("/login");
    },
  });

  if (me.isPending) {
    return (
      <Space size={4}>
        <Spin size="small" />
      </Space>
    );
  }

  if (me.isError || !me.data) {
    return <>{fallback}</>;
  }

  return (
    <Dropdown
      menu={{
        items: [
          {
            key: "profile",
            icon: <UserOutlined />,
            label: (
              <Link to="/profile">
                <Trans>个人资料</Trans>
              </Link>
            ),
          },
          {
            key: "logout",
            icon: <LogoutOutlined />,
            label: <Trans>退出登录</Trans>,
            onClick: () => logout.mutate(),
          },
        ],
      }}
    >
      <Space>{me.data.user.display_name ?? me.data.user.email}</Space>
    </Dropdown>
  );
}
