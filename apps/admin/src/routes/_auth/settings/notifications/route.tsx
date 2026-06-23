import { PageContainer } from "@ant-design/pro-components";
import { useLingui } from "@lingui/react/macro";
import { createFileRoute, Outlet, useNavigate, useRouterState } from "@tanstack/react-router";

export const Route = createFileRoute("/_auth/settings/notifications")({
  component: NotificationsLayout,
});

function NotificationsLayout() {
  const { t } = useLingui();
  const navigate = useNavigate();
  // 响应式订阅 location——useRouter().state 不是响应式的,切 tab 时 activeTab 会卡旧值。
  const pathname = useRouterState({ select: (s) => s.location.pathname });

  const activeTab = pathname.endsWith("/subscriptions")
    ? "subscriptions"
    : pathname.endsWith("/deliveries")
      ? "deliveries"
      : "endpoints";

  return (
    <PageContainer
      title={t`通知`}
      breadcrumbRender={false}
      tabActiveKey={activeTab}
      tabList={[
        { key: "endpoints", tab: t`Endpoints` },
        { key: "subscriptions", tab: t`Subscriptions` },
        { key: "deliveries", tab: t`Deliveries` },
      ]}
      onTabChange={(key) => {
        if (key === "endpoints") {
          navigate({ to: "/settings/notifications" });
        } else if (key === "subscriptions") {
          navigate({ to: "/settings/notifications/subscriptions" });
        } else if (key === "deliveries") {
          navigate({ to: "/settings/notifications/deliveries" });
        }
      }}
    >
      <Outlet />
    </PageContainer>
  );
}
