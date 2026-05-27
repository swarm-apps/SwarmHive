import { EyeOutlined, ReloadOutlined, SaveOutlined } from "@ant-design/icons";
import { Trans, useLingui } from "@lingui/react/macro";
import Editor from "@monaco-editor/react";
import { useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { App, Button, Card, Col, Input, List, Popconfirm, Row, Space, Tabs, Tag } from "antd";
import { useEffect, useMemo, useState } from "react";
import { fetchClient, isApiError } from "@/lib/api";
import { type MailTemplate, mailTemplatesQueryOptions, type PreviewResp } from "@/lib/api/mail";

export const Route = createFileRoute("/_auth/settings/mail/templates")({
  loader: ({ context }) => context.queryClient.ensureQueryData(mailTemplatesQueryOptions()),
  component: MailTemplatesPage,
});

const SAMPLE_CONTEXT = {
  user_name: "Alice",
  reset_link: "https://swarmhive.dev/reset?token=demo",
  invite_link: "https://swarmhive.dev/invite?token=demo",
  org_name: "Acme Inc.",
  invited_by: "Bob",
};

function MailTemplatesPage() {
  const { t } = useLingui();
  const { notification, modal } = App.useApp();
  const queryClient = useQueryClient();
  const { data: templates } = useSuspenseQuery(mailTemplatesQueryOptions());

  const [selectedId, setSelectedId] = useState<string | null>(templates[0]?.id ?? null);
  const selected = useMemo(
    () => templates.find((t) => t.id === selectedId) ?? null,
    [templates, selectedId],
  );

  // Local edit buffer — only persisted via the Save button.
  const [subject, setSubject] = useState(selected?.subject ?? "");
  const [htmlBody, setHtmlBody] = useState(selected?.html_body ?? "");
  const [textBody, setTextBody] = useState(selected?.text_body ?? "");
  const [activeTab, setActiveTab] = useState<"html" | "text" | "subject">("html");
  const [preview, setPreview] = useState<PreviewResp | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  // Reset buffer whenever the user picks a different template row.
  useEffect(() => {
    setSubject(selected?.subject ?? "");
    setHtmlBody(selected?.html_body ?? "");
    setTextBody(selected?.text_body ?? "");
    setPreview(null);
    setPreviewError(null);
  }, [selected]);

  const handleSave = async () => {
    if (!selected) return;
    setSaving(true);
    try {
      const { error } = await fetchClient.PUT("/api/v1/mail/templates/{id}", {
        params: { path: { id: selected.id } },
        body: { subject, html_body: htmlBody, text_body: textBody },
      });
      if (error) {
        throw error;
      }
      notification.success({ message: t`模板已保存` });
      await queryClient.invalidateQueries({ queryKey: mailTemplatesQueryOptions().queryKey });
    } catch (err) {
      const detail = isApiError(err) ? err.detail : String(err);
      notification.error({ message: t`保存失败`, description: detail });
    } finally {
      setSaving(false);
    }
  };

  const handlePreview = async () => {
    if (!selected) return;
    setPreviewError(null);
    try {
      const { data, error } = await fetchClient.POST("/api/v1/mail/templates/{id}/preview", {
        params: { path: { id: selected.id } },
        body: { sample: SAMPLE_CONTEXT },
      });
      if (error) {
        throw error;
      }
      setPreview(data ?? null);
    } catch (err) {
      const detail = (isApiError(err) ? err.detail : null) ?? String(err);
      const field = isApiError(err) ? err.extra<string>("field") : undefined;
      setPreviewError(field ? `[${field}] ${detail}` : detail);
      setPreview(null);
    }
  };

  const handleRestoreDefaults = () => {
    modal.confirm({
      title: t`恢复默认模板？`,
      content: t`所有 8 个内置模板将被覆盖为默认内容，自定义改动会丢失。`,
      okText: t`恢复`,
      okButtonProps: { danger: true },
      onOk: async () => {
        try {
          const { error } = await fetchClient.POST("/api/v1/mail/templates/seed-defaults");
          if (error) {
            throw error;
          }
          notification.success({ message: t`默认模板已恢复` });
          await queryClient.invalidateQueries({ queryKey: mailTemplatesQueryOptions().queryKey });
        } catch (err) {
          const detail = isApiError(err) ? err.detail : String(err);
          notification.error({ message: t`恢复失败`, description: detail });
        }
      },
    });
  };

  return (
    <Row gutter={16}>
      <Col span={8}>
        <Card
          size="small"
          title={<Trans>模板列表</Trans>}
          extra={
            <Popconfirm title={t`恢复默认模板？`} onConfirm={handleRestoreDefaults}>
              <Button size="small" icon={<ReloadOutlined />}>
                <Trans>恢复默认</Trans>
              </Button>
            </Popconfirm>
          }
        >
          <List<MailTemplate>
            dataSource={templates}
            renderItem={(item) => (
              <List.Item
                onClick={() => setSelectedId(item.id)}
                style={{
                  cursor: "pointer",
                  background: item.id === selectedId ? "rgba(22,119,255,0.08)" : undefined,
                  padding: "8px 12px",
                  borderRadius: 4,
                }}
              >
                <List.Item.Meta
                  title={item.event_name}
                  description={
                    <Space>
                      <Tag>{item.locale}</Tag>
                      <span style={{ color: "rgba(0,0,0,0.45)" }}>{item.subject}</span>
                    </Space>
                  }
                />
              </List.Item>
            )}
          />
        </Card>
      </Col>
      <Col span={16}>
        <Card
          size="small"
          title={selected ? `${selected.event_name} · ${selected.locale}` : <Trans>选择模板</Trans>}
          extra={
            selected && (
              <Space>
                <Button icon={<EyeOutlined />} onClick={handlePreview}>
                  <Trans>预览</Trans>
                </Button>
                <Button
                  type="primary"
                  icon={<SaveOutlined />}
                  loading={saving}
                  onClick={handleSave}
                >
                  <Trans>保存</Trans>
                </Button>
              </Space>
            )
          }
        >
          {selected ? (
            <>
              <Tabs
                activeKey={activeTab}
                onChange={(key) => setActiveTab(key as typeof activeTab)}
                items={[
                  {
                    key: "subject",
                    label: t`Subject`,
                    children: (
                      <Input
                        value={subject}
                        onChange={(e) => setSubject(e.target.value)}
                        placeholder={t`邮件标题（支持 minijinja 表达式）`}
                      />
                    ),
                  },
                  {
                    key: "html",
                    label: t`HTML`,
                    children: (
                      <Editor
                        height="400px"
                        defaultLanguage="html"
                        value={htmlBody}
                        onChange={(value) => setHtmlBody(value ?? "")}
                        options={{ minimap: { enabled: false }, fontSize: 13 }}
                      />
                    ),
                  },
                  {
                    key: "text",
                    label: t`Text`,
                    children: (
                      <Editor
                        height="400px"
                        defaultLanguage="plaintext"
                        value={textBody}
                        onChange={(value) => setTextBody(value ?? "")}
                        options={{ minimap: { enabled: false }, fontSize: 13 }}
                      />
                    ),
                  },
                ]}
              />
              {previewError && (
                <Card size="small" style={{ marginTop: 12, borderColor: "#ff4d4f" }}>
                  <Trans>预览失败：</Trans>
                  <pre style={{ margin: 0, color: "#cf1322" }}>{previewError}</pre>
                </Card>
              )}
              {preview && (
                <Card size="small" style={{ marginTop: 12 }} title={`Subject: ${preview.subject}`}>
                  <iframe
                    title="preview"
                    srcDoc={preview.html_body}
                    style={{
                      width: "100%",
                      height: 400,
                      border: "1px solid rgba(0,0,0,0.1)",
                      background: "white",
                    }}
                    sandbox=""
                  />
                </Card>
              )}
            </>
          ) : (
            <div style={{ padding: 24, textAlign: "center", color: "rgba(0,0,0,0.45)" }}>
              <Trans>从左侧选择一个模板进行编辑</Trans>
            </div>
          )}
        </Card>
      </Col>
    </Row>
  );
}
