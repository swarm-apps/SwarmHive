import { Column, Line } from "@ant-design/plots";
import { PageContainer } from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { Card, Col, Empty, Row, Segmented, Select, Statistic, Table, Tooltip } from "antd";
import { useMemo, useState } from "react";
import { appsQueryOptions } from "@/lib/api/apps";
import {
  type AdoptionPoint,
  type FunnelStage,
  telemetryAdoptionQueryOptions,
  telemetryDistributionQueryOptions,
  telemetryFunnelQueryOptions,
  telemetrySummaryQueryOptions,
} from "@/lib/api/telemetry";

export const Route = createFileRoute("/_auth/telemetry")({
  component: TelemetryPage,
});

const STAGE_LABELS: Record<string, string> = {
  check_available: "有更新的检查",
  download_redirected: "下载重定向",
  download_completed: "下载完成",
  started_after_update: "更新后启动",
};

function TelemetryPage() {
  const { t } = useLingui();
  const apps = useQuery(appsQueryOptions()).data ?? [];
  const [appSlug, setAppSlug] = useState<string>("");
  const [days, setDays] = useState<number>(30);
  // app 列表就绪后默认选第一个。
  const app = appSlug || apps[0]?.slug || "";

  const summary = useQuery(telemetrySummaryQueryOptions(app, days)).data;
  const adoption = useQuery(telemetryAdoptionQueryOptions(app, days)).data ?? [];
  const funnel = useQuery(telemetryFunnelQueryOptions(app, days)).data ?? [];
  const platformDist = useQuery(telemetryDistributionQueryOptions(app, days, "platform")).data;
  const archDist = useQuery(telemetryDistributionQueryOptions(app, days, "arch")).data;

  // adoption:version=null 是当日总活跃行,曲线只画 per-version 序列。
  const adoptionSeries = useMemo(
    () => adoption.filter((p: AdoptionPoint) => p.version != null),
    [adoption],
  );
  // 版本长尾:期内每版本最近一天的活跃设备(取各版本最后一个数据点)。
  const versionTail = useMemo(() => {
    const last = new Map<string, AdoptionPoint>();
    for (const p of adoptionSeries) {
      if (p.version) last.set(p.version, p);
    }
    return [...last.values()].sort((a, b) => b.unique_clients - a.unique_clients);
  }, [adoptionSeries]);

  const hasData = adoptionSeries.length > 0 || funnel.some((s: FunnelStage) => s.count > 0);

  return (
    <PageContainer
      title={t`统计`}
      subTitle={t`更新链路观测(day 桶为 UTC;漏斗按次计数,非设备去重)`}
      breadcrumbRender={false}
      extra={[
        <Select
          key="app"
          style={{ width: 200 }}
          value={app || undefined}
          placeholder={t`选择应用`}
          onChange={setAppSlug}
          options={apps.map((a) => ({ label: a.display_name, value: a.slug }))}
        />,
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
      ]}
    >
      <Row gutter={16}>
        <Col span={8}>
          <Card>
            <Statistic title={t`今日活跃设备`} value={summary?.active_devices_today ?? 0} />
          </Card>
        </Col>
        <Col span={8}>
          <Card>
            <Statistic title={t`期内下载完成`} value={summary?.downloads_completed ?? 0} />
          </Card>
        </Col>
        <Col span={8}>
          <Card>
            <Tooltip title={t`最新已发布版本的活跃设备占今日总活跃的比例`}>
              <Statistic
                title={
                  summary?.latest_version
                    ? t`最新版 ${summary.latest_version} Active%`
                    : t`最新版 Active%`
                }
                value={summary?.latest_version_active_pct ?? 0}
                suffix="%"
              />
            </Tooltip>
          </Card>
        </Col>
      </Row>

      {!hasData ? (
        <Card style={{ marginTop: 16 }}>
          <Empty
            description={
              <Trans>
                还没有遥测数据。客户端发起更新检查、或 SDK 接入事件上报后,这里会出现
                版本采用曲线与更新漏斗(rollup 每小时聚合一次)。
              </Trans>
            }
          />
        </Card>
      ) : (
        <>
          <Card title={t`版本采用曲线(每日活跃设备,按版本)`} style={{ marginTop: 16 }}>
            <Line
              data={adoptionSeries}
              xField="day"
              yField="unique_clients"
              colorField="version"
              height={300}
              axis={{ y: { title: t`活跃设备` } }}
            />
          </Card>

          <Row gutter={16} style={{ marginTop: 16 }}>
            <Col span={12}>
              <Card title={t`更新漏斗(按次计数)`}>
                <Column
                  data={funnel.map((s: FunnelStage) => ({
                    stage: STAGE_LABELS[s.stage] ?? s.stage,
                    count: s.count,
                  }))}
                  xField="stage"
                  yField="count"
                  height={280}
                />
                <Table
                  size="small"
                  style={{ marginTop: 12 }}
                  pagination={false}
                  rowKey="stage"
                  dataSource={funnel}
                  columns={[
                    {
                      title: t`阶段`,
                      dataIndex: "stage",
                      render: (v: string) => STAGE_LABELS[v] ?? v,
                    },
                    { title: t`次数`, dataIndex: "count" },
                    {
                      title: t`转化率`,
                      dataIndex: "conversion_pct",
                      render: (v: number | null) => (v == null ? "-" : `${v}%`),
                    },
                  ]}
                />
              </Card>
            </Col>
            <Col span={12}>
              <Card title={t`检查请求分布`}>
                <Column
                  data={(platformDist ?? []).map((d) => ({ ...d, dim: t`平台` }))}
                  xField="key"
                  yField="count"
                  height={140}
                />
                <Column
                  data={(archDist ?? []).map((d) => ({ ...d, dim: "arch" }))}
                  xField="key"
                  yField="count"
                  height={140}
                />
              </Card>
            </Col>
          </Row>

          <Card title={t`版本长尾(各版本最近一日活跃设备)`} style={{ marginTop: 16 }}>
            <Table
              size="small"
              pagination={false}
              rowKey="version"
              dataSource={versionTail}
              columns={[
                { title: t`版本`, dataIndex: "version" },
                { title: t`活跃设备`, dataIndex: "unique_clients" },
                { title: t`数据日期`, dataIndex: "day" },
              ]}
            />
          </Card>
        </>
      )}
    </PageContainer>
  );
}
