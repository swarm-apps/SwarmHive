import { PageContainer, ProTable } from "@ant-design/pro-components";
import { useLingui } from "@lingui/react/macro";
import { createFileRoute } from "@tanstack/react-router";
import { Tag } from "antd";

interface ReleaseRow {
  id: string;
  app: string;
  version: string;
  channel: string;
  status: "draft" | "published" | "yanked";
  publishedAt?: string;
}

export const Route = createFileRoute("/_auth/releases")({
  component: ReleasesPage,
});

function ReleasesPage() {
  const { t } = useLingui();
  return (
    <PageContainer title={t`版本`}>
      <ProTable<ReleaseRow>
        rowKey="id"
        search={false}
        toolBarRender={() => []}
        request={async () => ({ data: [], success: true, total: 0 })}
        columns={[
          { title: t`应用`, dataIndex: "app" },
          { title: t`版本号`, dataIndex: "version" },
          { title: t`Channel`, dataIndex: "channel" },
          {
            title: t`状态`,
            dataIndex: "status",
            render: (_, row) => {
              const color =
                row.status === "published" ? "green" : row.status === "draft" ? "default" : "red";
              return <Tag color={color}>{row.status}</Tag>;
            },
          },
          { title: t`发布时间`, dataIndex: "publishedAt" },
        ]}
      />
    </PageContainer>
  );
}
