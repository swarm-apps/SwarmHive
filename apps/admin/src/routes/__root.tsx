import {
  AppstoreOutlined,
  DashboardOutlined,
  LogoutOutlined,
  RocketOutlined,
} from "@ant-design/icons";
import { ProLayout } from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import type { QueryClient } from "@tanstack/react-query";
import { useQuery } from "@tanstack/react-query";
import { createRootRouteWithContext, Link, Outlet, useRouter } from "@tanstack/react-router";
import { Dropdown, Space, Spin } from "antd";
import { lazy, Suspense } from "react";
import { meQueryOptions } from "@/lib/query/meQuery";
import { ColorModeToggle, useColorModeContext } from "@/lib/theme";

const TanStackRouterDevtools = import.meta.env.DEV
  ? lazy(() =>
      import("@tanstack/router-devtools").then((m) => ({ default: m.TanStackRouterDevtools })),
    )
  : () => null;

interface RouterContext {
  queryClient: QueryClient;
}

export const Route = createRootRouteWithContext<RouterContext>()({
  component: RootLayout,
});

function RootLayout() {
  const router = useRouter();
  const pathname = router.state.location.pathname;
  const { resolved } = useColorModeContext();
  const { t } = useLingui();

  return (
    <ProLayout
      title="SwarmHive"
      logo={false}
      layout="mix"
      fixSiderbar
      fixedHeader
      navTheme={resolved === "dark" ? "realDark" : "light"}
      location={{ pathname }}
      menuItemRender={(item, dom) => <Link to={item.path ?? "/"}>{dom}</Link>}
      route={{
        path: "/",
        routes: [
          { path: "/", name: t`仪表盘`, icon: <DashboardOutlined /> },
          { path: "/apps", name: t`应用`, icon: <AppstoreOutlined /> },
          { path: "/releases", name: t`版本`, icon: <RocketOutlined /> },
        ],
      }}
      actionsRender={() => [<ColorModeToggle key="color-mode" />]}
      avatarProps={{
        render: (_props, dom) => <UserAvatar fallback={dom} />,
      }}
    >
      <Outlet />
      {import.meta.env.DEV ? (
        <Suspense fallback={null}>
          <TanStackRouterDevtools />
        </Suspense>
      ) : null}
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
