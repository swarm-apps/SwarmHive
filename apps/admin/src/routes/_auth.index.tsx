import { Line } from "@ant-design/charts";
import { PageContainer, ProCard, StatisticCard } from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_auth/")({
  component: DashboardPage,
});

const PLACEHOLDER_TREND = [
  { date: "2026-05-19", value: 0 },
  { date: "2026-05-20", value: 0 },
  { date: "2026-05-21", value: 0 },
  { date: "2026-05-22", value: 0 },
  { date: "2026-05-23", value: 0 },
  { date: "2026-05-24", value: 0 },
  { date: "2026-05-25", value: 0 },
];

function DashboardPage() {
  const { t } = useLingui();
  return (
    <PageContainer title={t`仪表盘`} subTitle={t`SwarmHive 自托管发布中枢`}>
      <ProCard split="vertical">
        <StatisticCard statistic={{ title: t`应用数`, value: 0 }} />
        <StatisticCard statistic={{ title: t`版本数`, value: 0 }} />
        <StatisticCard statistic={{ title: t`今日下载`, value: 0 }} />
        <StatisticCard statistic={{ title: t`更新检查`, value: 0 }} />
      </ProCard>
      <ProCard title={<Trans>下载趋势（占位）</Trans>} style={{ marginTop: 16 }}>
        <Line
          data={PLACEHOLDER_TREND}
          xField="date"
          yField="value"
          height={220}
          autoFit
          point={{ size: 4 }}
        />
      </ProCard>
      <ProCard style={{ marginTop: 16 }}>
        <Trans>
          欢迎使用 SwarmHive 管理后台。具体的应用、版本、Token、用户管理模块由后续 proposal 落地。
        </Trans>
      </ProCard>
    </PageContainer>
  );
}
