import { PageContainer, ProTable } from "@ant-design/pro-components";
import { useLingui } from "@lingui/react/macro";
import { createFileRoute } from "@tanstack/react-router";

interface AppRow {
  id: string;
  name: string;
  slug: string;
  platform: string;
  defaultChannel: string;
}

export const Route = createFileRoute("/_auth/apps")({
  component: AppsPage,
});

function AppsPage() {
  const { t } = useLingui();
  return (
    <PageContainer title={t`应用`}>
      <ProTable<AppRow>
        rowKey="id"
        search={false}
        toolBarRender={() => []}
        request={async () => ({ data: [], success: true, total: 0 })}
        columns={[
          { title: t`名称`, dataIndex: "name" },
          { title: t`Slug`, dataIndex: "slug" },
          { title: t`平台`, dataIndex: "platform" },
          { title: t`默认 Channel`, dataIndex: "defaultChannel" },
        ]}
      />
    </PageContainer>
  );
}
