import { type ActionType, type ProColumns, ProTable } from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { App, Button, Descriptions, Select, Spin, Tag } from "antd";
import dayjs from "dayjs";
import { useRef } from "react";
import { z } from "zod";
import { fetchClient, isApiError } from "@/lib/api";
import {
  DELIVERIES_PATH,
  type Delivery,
  type DeliveryStatus,
  deliveryDetailQueryOptions,
  deliveryStatusColor,
  endpointName,
  endpointsQueryOptions,
  redeliver,
} from "@/lib/api/notifications";

const DELIVERY_STATUSES = ["pending", "sent", "failed", "dead"] as const;

export const Route = createFileRoute("/_auth/settings/notifications/deliveries")({
  // ?endpoint= 由 Endpoints tab 的「查看投递」带入;?status= 由本页过滤器写入。
  // URL 持有过滤态 → 可分享 / 刷新保留(URL + Query 范式,同 releases 的 ?app=)。
  validateSearch: z.object({
    endpoint: z.string().optional(),
    status: z.enum(DELIVERY_STATUSES).optional(),
  }),
  component: DeliveriesPage,
});

function DeliveriesPage() {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const { endpoint, status } = Route.useSearch();
  const navigate = Route.useNavigate();
  const tableRef = useRef<ActionType | null>(null);

  const endpointsQuery = useQuery(endpointsQueryOptions());
  const endpoints = endpointsQuery.data ?? [];

  const setFilter = (patch: { endpoint?: string; status?: DeliveryStatus }) =>
    navigate({ search: (prev) => ({ ...prev, ...patch }) });

  const handleRedeliver = async (row: Delivery) => {
    try {
      await redeliver(row.id);
      // 重投保持原 webhook-id(后端约束),接收端据此幂等去重。非破坏,无需二次确认。
      notification.success({ message: t`已重新入队`, description: t`保持原 webhook-id` });
      tableRef.current?.reload();
    } catch (error) {
      notification.error({
        message: t`重投失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
    }
  };

  const columns: ProColumns<Delivery>[] = [
    {
      title: t`状态`,
      width: 100,
      render: (_, row) => <Tag color={deliveryStatusColor(row.status)}>{row.status}</Tag>,
    },
    { title: t`事件`, dataIndex: "event_type" },
    {
      title: t`Endpoint`,
      render: (_, row) =>
        endpointName(endpoints, row.webhook_endpoint_id) ??
        (row.channel_kind === "email" ? t`邮件` : "—"),
    },
    {
      title: t`响应码`,
      width: 90,
      render: (_, row) =>
        row.response_code != null ? (
          <code
            style={{
              fontFamily: "monospace",
              color: row.response_code < 400 ? "#389e0d" : "#cf1322",
            }}
          >
            {row.response_code}
          </code>
        ) : (
          "—"
        ),
    },
    { title: t`尝试`, dataIndex: "attempt", width: 70 },
    {
      title: t`更新时间`,
      dataIndex: "updated_at",
      width: 180,
      render: (_, row) => dayjs(row.updated_at).format("YYYY-MM-DD HH:mm:ss"),
    },
    {
      title: t`下次重试`,
      width: 180,
      // 终态(dead)/ 成功(sent)无 next_retry_at;failed/pending 才有。
      render: (_, row) =>
        row.next_retry_at ? dayjs(row.next_retry_at).format("YYYY-MM-DD HH:mm:ss") : "—",
    },
    {
      title: t`操作`,
      width: 80,
      render: (_, row) => (
        <Button type="link" size="small" onClick={() => handleRedeliver(row)}>
          <Trans>重投</Trans>
        </Button>
      ),
    },
  ];

  return (
    <ProTable<Delivery>
      actionRef={tableRef}
      rowKey="id"
      search={false}
      options={false}
      // params 变化(过滤器改 URL search)触发 ProTable 重新 request。
      params={{ endpoint, status }}
      pagination={{ pageSize: 50 }}
      expandable={{
        // 懒加载详情:仅行展开时拉 GET /deliveries/{id} 取请求/响应快照。
        expandedRowRender: (row) => <DeliveryDetailPanel id={row.id} />,
      }}
      toolBarRender={() => [
        <Select
          key="endpoint"
          allowClear
          placeholder={t`按 Endpoint 过滤`}
          style={{ width: 200 }}
          value={endpoint}
          onChange={(v?: string) => setFilter({ endpoint: v })}
          options={endpoints.map((e) => ({ label: e.name, value: e.id }))}
        />,
        <Select
          key="status"
          allowClear
          placeholder={t`按状态过滤`}
          style={{ width: 140 }}
          value={status}
          onChange={(v?: DeliveryStatus) => setFilter({ status: v })}
          options={DELIVERY_STATUSES.map((s) => ({ label: s, value: s }))}
        />,
      ]}
      request={async () => {
        const { data, error } = await fetchClient.GET(DELIVERIES_PATH, {
          params: { query: { webhook_endpoint_id: endpoint, status, limit: 200 } },
        });
        if (error) throw error;
        return { data: data ?? [], success: true, total: data?.length ?? 0 };
      }}
      columns={columns}
    />
  );
}

/** 行展开面板:懒加载投递详情,展示 Request(签名头 + body)与 Response(code + body)。 */
function DeliveryDetailPanel({ id }: { id: string }) {
  const { t } = useLingui();
  const detail = useQuery(deliveryDetailQueryOptions(id, true));

  if (detail.isPending) {
    return <Spin size="small" />;
  }
  if (detail.isError) {
    return (
      <span style={{ color: "#cf1322" }}>
        {isApiError(detail.error) ? detail.error.detail : String(detail.error)}
      </span>
    );
  }

  const d = detail.data;
  const hasSnapshot = d.request_body != null || d.response_body != null;
  const preStyle: React.CSSProperties = {
    margin: "4px 0 0",
    whiteSpace: "pre-wrap",
    wordBreak: "break-all",
    maxHeight: 240,
    overflow: "auto",
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      {d.delivery.last_error && (
        <pre style={{ ...preStyle, color: "#cf1322" }}>{d.delivery.last_error}</pre>
      )}
      {hasSnapshot ? (
        <>
          <Descriptions
            size="small"
            column={1}
            title={t`请求`}
            items={[
              {
                key: "id",
                label: "webhook-id",
                children: <code>{d.delivery.event_id}</code>,
              },
              {
                key: "ts",
                label: "webhook-timestamp",
                children: d.request_timestamp ?? "—",
              },
              {
                key: "sig",
                label: "webhook-signature",
                children: (
                  <code style={{ wordBreak: "break-all" }}>{d.request_signature ?? "—"}</code>
                ),
              },
            ]}
          />
          {d.request_body && <pre style={preStyle}>{prettyJson(d.request_body)}</pre>}
          <Descriptions
            size="small"
            column={1}
            title={t`响应`}
            items={[{ key: "code", label: t`状态码`, children: d.delivery.response_code ?? "—" }]}
          />
          {d.response_body && <pre style={preStyle}>{d.response_body}</pre>}
        </>
      ) : (
        <span style={{ color: "rgba(0,0,0,0.45)" }}>
          {d.delivery.channel_kind === "email" ? (
            <Trans>邮件投递无 HTTP 请求 / 响应快照。</Trans>
          ) : (
            <Trans>暂无请求 / 响应快照（可能尚未投递）。</Trans>
          )}
        </span>
      )}
      {(d.attempts?.length ?? 0) > 0 && (
        <div>
          <div style={{ fontWeight: 500, marginBottom: 4 }}>
            <Trans>尝试时间线</Trans>
          </div>
          {(d.attempts ?? []).map((a) => (
            <div
              key={a.id}
              style={{ display: "flex", gap: 8, alignItems: "baseline", margin: "2px 0" }}
            >
              <span style={{ color: "rgba(0,0,0,0.45)" }}>#{a.attempt_no}</span>
              <Tag color={deliveryStatusColor(a.status)}>{a.status}</Tag>
              <code style={{ fontFamily: "monospace" }}>{a.response_code ?? "—"}</code>
              <span style={{ color: "rgba(0,0,0,0.45)" }}>
                {dayjs(a.created_at).format("YYYY-MM-DD HH:mm:ss")}
              </span>
              {a.last_error && (
                <span style={{ color: "#cf1322", wordBreak: "break-all" }}>{a.last_error}</span>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/** 尽力把 JSON 字符串美化缩进;解析失败则原样返回。 */
function prettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}
