import { ProTable } from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { createFileRoute } from "@tanstack/react-router";
import { Tag } from "antd";
import dayjs from "dayjs";
import { fetchClient } from "@/lib/api";
import { LOGS_PATH, type MailLog } from "@/lib/api/mail";

export const Route = createFileRoute("/_auth/settings/mail/logs")({
  component: MailLogsPage,
});

function MailLogsPage() {
  const { t } = useLingui();

  return (
    <ProTable<MailLog>
      rowKey="id"
      search={{ filterType: "light" }}
      pagination={{ pageSize: 50 }}
      expandable={{
        expandedRowRender: (row) =>
          row.error ? (
            <pre style={{ margin: 0, whiteSpace: "pre-wrap", color: "#cf1322" }}>{row.error}</pre>
          ) : (
            <span style={{ color: "rgba(0,0,0,0.45)" }}>
              <Trans>无错误信息</Trans>
            </span>
          ),
        rowExpandable: (row) => Boolean(row.error),
      }}
      request={async (params) => {
        const limit = params.pageSize ?? 50;
        const { data, error } = await fetchClient.GET(LOGS_PATH, {
          params: { query: { limit } },
        });
        if (error) {
          throw error;
        }
        let rows = data ?? [];
        // Client-side filtering until the server endpoint grows query params
        // (left out of the MVP — Postgres LIKE on `to` + status equality both
        // trivial follow-ups).
        if (params.to) {
          const needle = String(params.to).toLowerCase();
          rows = rows.filter((r) => r.to.toLowerCase().includes(needle));
        }
        if (params.status) {
          rows = rows.filter((r) => r.status === params.status);
        }
        return { data: rows, success: true, total: rows.length };
      }}
      columns={[
        {
          title: t`发送时间`,
          dataIndex: "sent_at",
          search: false,
          render: (_, row) => dayjs(row.sent_at).format("YYYY-MM-DD HH:mm:ss"),
          width: 180,
        },
        {
          title: t`收件人`,
          dataIndex: "to",
        },
        {
          title: t`Template`,
          dataIndex: "template_id",
          search: false,
          render: (_, row) => row.template_id ?? "-",
        },
        {
          title: t`状态`,
          dataIndex: "status",
          valueEnum: {
            sent: { text: t`已发送`, status: "Success" },
            failed: { text: t`失败`, status: "Error" },
          },
          render: (_, row) =>
            row.status === "sent" ? (
              <Tag color="success">
                <Trans>已发送</Trans>
              </Tag>
            ) : (
              <Tag color="error">
                <Trans>失败</Trans>
              </Tag>
            ),
        },
        {
          title: t`错误摘要`,
          dataIndex: "error",
          search: false,
          ellipsis: true,
          render: (_, row) => row.error ?? "-",
        },
      ]}
    />
  );
}
