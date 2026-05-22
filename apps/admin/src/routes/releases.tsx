import { PageContainer, ProTable } from "@ant-design/pro-components";
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

export const Route = createFileRoute("/releases")({
  component: ReleasesPage,
});

function ReleasesPage() {
  return (
    <PageContainer title="版本">
      <ProTable<ReleaseRow>
        rowKey="id"
        search={false}
        toolBarRender={() => []}
        request={async () => ({ data: [], success: true, total: 0 })}
        columns={[
          { title: "应用", dataIndex: "app" },
          { title: "版本号", dataIndex: "version" },
          { title: "Channel", dataIndex: "channel" },
          {
            title: "状态",
            dataIndex: "status",
            render: (_, row) => {
              const color =
                row.status === "published" ? "green" : row.status === "draft" ? "default" : "red";
              return <Tag color={color}>{row.status}</Tag>;
            },
          },
          { title: "发布时间", dataIndex: "publishedAt" },
        ]}
      />
    </PageContainer>
  );
}
