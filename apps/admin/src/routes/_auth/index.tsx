import { Line } from "@ant-design/plots";
import { PageContainer, ProCard, StatisticCard } from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { Empty, Segmented } from "antd";
import { useMemo, useState } from "react";
import { telemetryOverviewQueryOptions } from "@/lib/api/telemetry";
import { usePermissions } from "@/lib/query/usePermissions";

export const Route = createFileRoute("/_auth/")({
  component: DashboardPage,
});

function DashboardPage() {
  const { t } = useLingui();
  const { has } = usePermissions();
  const canRead = has("telemetry:read");
  const [days, setDays] = useState<number>(30);
  // 无 telemetry:read 的角色不发请求(viewer 默认有此权限);整页降级为权限提示,
  // 不展示误导性的 0 卡。
  const overview = useQuery(telemetryOverviewQueryOptions(days, canRead));
  const data = overview.data;

  // 趋势转长表:每天两条(更新检查 / 下载完成),colorField 分系列。
  const trendData = useMemo(() => {
    const rows: { day: string; type: string; value: number }[] = [];
    for (const p of data?.trend ?? []) {
      rows.push({ day: p.day, type: t`更新检查`, value: p.update_checks });
      rows.push({ day: p.day, type: t`下载完成`, value: p.downloads_completed });
    }
    return rows;
  }, [data, t]);

  return (
    <PageContainer
      title={t`仪表盘`}
      subTitle={t`SwarmHive 自托管发布中枢`}
      breadcrumbRender={false}
      extra={
        canRead
          ? [
              <Segmented
                key="days"
                value={days}
                onChange={(v) => setDays(v as number)}
                options={[
                  { label: t`7 天`, value: 7 },
                  { label: t`30 天`, value: 30 },
                  { label: t`90 天`, value: 90 },
                ]}
              />,
            ]
          : undefined
      }
    >
      {!canRead ? (
        <ProCard>
          <Empty description={<Trans>需要 telemetry:read 权限查看仪表盘统计。</Trans>} />
        </ProCard>
      ) : (
        <>
          <ProCard split="vertical">
            <StatisticCard
              loading={overview.isLoading}
              statistic={{ title: t`应用数`, value: data?.app_count ?? 0 }}
            />
            <StatisticCard
              loading={overview.isLoading}
              statistic={{ title: t`版本数`, value: data?.release_count ?? 0 }}
            />
            <StatisticCard
              loading={overview.isLoading}
              statistic={{ title: t`${days} 天更新检查`, value: data?.update_checks ?? 0 }}
            />
            <StatisticCard
              loading={overview.isLoading}
              statistic={{ title: t`${days} 天下载完成`, value: data?.downloads_completed ?? 0 }}
            />
          </ProCard>

          <ProCard
            title={<Trans>活动趋势（每日更新检查 / 下载完成）</Trans>}
            style={{ marginTop: 16 }}
            loading={overview.isLoading}
          >
            {trendData.length > 0 ? (
              <Line
                data={trendData}
                xField="day"
                yField="value"
                colorField="type"
                height={260}
                axis={{ y: { title: t`次数` } }}
              />
            ) : (
              <Empty
                description={
                  <Trans>
                    所选时间窗内还没有更新检查 / 下载事件。客户端接入更新链路后,这里会出现活动
                    曲线(rollup 每小时聚合一次)。
                  </Trans>
                }
              />
            )}
          </ProCard>
        </>
      )}
    </PageContainer>
  );
}
