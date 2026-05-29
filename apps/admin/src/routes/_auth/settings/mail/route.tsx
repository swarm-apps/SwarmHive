import { PageContainer } from "@ant-design/pro-components";
import { useLingui } from "@lingui/react/macro";
import { createFileRoute, Outlet, useNavigate, useRouterState } from "@tanstack/react-router";

export const Route = createFileRoute("/_auth/settings/mail")({
  component: MailLayout,
});

function MailLayout() {
  const { t } = useLingui();
  const navigate = useNavigate();
  // 响应式订阅 location——useRouter().state 不是响应式的，切 tab 时 activeTab 会卡在旧值。
  const pathname = useRouterState({ select: (s) => s.location.pathname });

  // Index route lives under /settings/mail; sibling pages append /templates or /logs.
  const activeTab = pathname.endsWith("/templates")
    ? "templates"
    : pathname.endsWith("/logs")
      ? "logs"
      : "providers";

  return (
    <PageContainer
      title={t`邮件`}
      breadcrumbRender={false}
      tabActiveKey={activeTab}
      tabList={[
        { key: "providers", tab: t`Providers` },
        { key: "templates", tab: t`Templates` },
        { key: "logs", tab: t`Logs` },
      ]}
      onTabChange={(key) => {
        if (key === "providers") {
          navigate({ to: "/settings/mail" });
        } else if (key === "templates") {
          navigate({ to: "/settings/mail/templates" });
        } else if (key === "logs") {
          navigate({ to: "/settings/mail/logs" });
        }
      }}
    >
      <Outlet />
    </PageContainer>
  );
}
