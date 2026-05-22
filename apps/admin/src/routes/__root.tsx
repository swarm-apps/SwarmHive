import { AppstoreOutlined, DashboardOutlined, RocketOutlined } from "@ant-design/icons";
import { ProLayout } from "@ant-design/pro-components";
import type { QueryClient } from "@tanstack/react-query";
import { createRootRouteWithContext, Link, Outlet, useRouter } from "@tanstack/react-router";

interface RouterContext {
  queryClient: QueryClient;
}

export const Route = createRootRouteWithContext<RouterContext>()({
  component: RootLayout,
});

function RootLayout() {
  const router = useRouter();
  const pathname = router.state.location.pathname;

  return (
    <ProLayout
      title="SwarmHive"
      logo={false}
      layout="mix"
      fixSiderbar
      fixedHeader
      location={{ pathname }}
      menuItemRender={(item, dom) => <Link to={item.path ?? "/"}>{dom}</Link>}
      route={{
        path: "/",
        routes: [
          {
            path: "/",
            name: "Dashboard",
            icon: <DashboardOutlined />,
          },
          {
            path: "/apps",
            name: "应用",
            icon: <AppstoreOutlined />,
          },
          {
            path: "/releases",
            name: "版本",
            icon: <RocketOutlined />,
          },
        ],
      }}
    >
      <Outlet />
    </ProLayout>
  );
}
