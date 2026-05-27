import { PageContainer } from "@ant-design/pro-components";
import { useLingui } from "@lingui/react/macro";
import { createFileRoute, Outlet, useNavigate, useRouter } from "@tanstack/react-router";

export const Route = createFileRoute("/_auth/settings/mail")({
  component: MailLayout,
});

function MailLayout() {
  const { t } = useLingui();
  const router = useRouter();
  const navigate = useNavigate();
  const pathname = router.state.location.pathname;

  // Index route lives under /settings/mail; sibling pages append /templates or /logs.
  const activeTab = pathname.endsWith("/templates")
    ? "templates"
    : pathname.endsWith("/logs")
      ? "logs"
      : "providers";

  return (
    <PageContainer
      title={t`邮件`}
      breadcrumb={undefined}
      header={{ breadcrumb: undefined }}
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
