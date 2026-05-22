import { PageContainer, ProCard, StatisticCard } from "@ant-design/pro-components";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/")({
  component: DashboardPage,
});

function DashboardPage() {
  return (
    <PageContainer title="Dashboard" subTitle="SwarmHive 自托管发布中枢">
      <ProCard split="vertical">
        <StatisticCard
          statistic={{
            title: "应用数",
            value: 0,
          }}
        />
        <StatisticCard
          statistic={{
            title: "版本数",
            value: 0,
          }}
        />
        <StatisticCard
          statistic={{
            title: "今日下载",
            value: 0,
          }}
        />
        <StatisticCard
          statistic={{
            title: "更新检查",
            value: 0,
          }}
        />
      </ProCard>
    </PageContainer>
  );
}
