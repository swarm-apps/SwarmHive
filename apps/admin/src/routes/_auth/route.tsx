import {
  AppstoreOutlined,
  BarChartOutlined,
  CloudOutlined,
  DashboardOutlined,
  LogoutOutlined,
  MailOutlined,
  RocketOutlined,
  SafetyOutlined,
  SettingOutlined,
} from "@ant-design/icons";
import { ProLayout } from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute, Link, Outlet, redirect, useRouter } from "@tanstack/react-router";
import { Alert, Dropdown, Space, Spin } from "antd";
import { isApiError } from "@/lib/api";
import { mailStatusQueryOptions } from "@/lib/api/mail";
import { meQueryOptions } from "@/lib/query/meQuery";
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
  const router = useRouter();
  const pathname = router.state.location.pathname;
  const { resolved } = useColorModeContext();
  const me = useQuery({ ...meQueryOptions(), retry: false });
  const mailStatus = useQuery({ ...mailStatusQueryOptions(), retry: false });

  const canManageSettings = me.data?.permissions.includes("mail:manage") ?? false;
  // Only nag operators in non-dev builds — local Vite dev defaults to the
  // mailpit provider so the banner would be noise.
  const showFallbackBanner = !import.meta.env.DEV && mailStatus.data?.fallback_mode === true;

  const settingsRoute = canManageSettings
    ? [
        {
          path: "/settings",
          name: t`设置`,
          icon: <SettingOutlined />,
          routes: [
            { path: "/settings/mail", name: t`邮件`, icon: <MailOutlined /> },
            {
              path: "/settings/auth",
              name: t`认证`,
              icon: <SafetyOutlined />,
              disabled: true,
            },
            {
              path: "/settings/storage",
              name: t`存储`,
              icon: <CloudOutlined />,
              disabled: true,
            },
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
          ...settingsRoute,
        ],
      }}
      actionsRender={() => [<ColorModeToggle key="color-mode" />]}
      avatarProps={{
        render: (_props, dom) => <UserAvatar fallback={dom} />,
      }}
    >
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

function UserAvatar({ fallback }: { fallback: React.ReactNode }) {
  const me = useQuery({ ...meQueryOptions(), retry: false });

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
            key: "logout",
            icon: <LogoutOutlined />,
            label: <Trans>退出登录</Trans>,
            onClick: () => {
              // 真正的 logout 调用由后续 auth UI proposal 接管
              window.location.assign("/login");
            },
          },
        ],
      }}
    >
      <Space>{me.data.user.display_name ?? me.data.user.email}</Space>
    </Dropdown>
  );
}
