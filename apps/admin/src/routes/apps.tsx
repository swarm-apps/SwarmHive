import { PageContainer, ProTable } from "@ant-design/pro-components";
import { createFileRoute } from "@tanstack/react-router";

interface AppRow {
  id: string;
  name: string;
  slug: string;
  platform: string;
  defaultChannel: string;
}

export const Route = createFileRoute("/apps")({
  component: AppsPage,
});

function AppsPage() {
  return (
    <PageContainer title="应用">
      <ProTable<AppRow>
        rowKey="id"
        search={false}
        toolBarRender={() => []}
        request={async () => ({ data: [], success: true, total: 0 })}
        columns={[
          { title: "名称", dataIndex: "name" },
          { title: "Slug", dataIndex: "slug" },
          { title: "平台", dataIndex: "platform" },
          { title: "默认 Channel", dataIndex: "defaultChannel" },
        ]}
      />
    </PageContainer>
  );
}
