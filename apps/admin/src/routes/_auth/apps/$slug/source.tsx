import { EditOutlined, PlusOutlined } from "@ant-design/icons";
import {
  DrawerForm,
  ProFormCheckbox,
  ProFormSwitch,
  ProFormText,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { Alert, App, Button, Card, Descriptions, Empty, Space, Tag, Typography } from "antd";
import { useState } from "react";
import { isApiError } from "@/lib/api";
import {
  type CreateGithubSourceRequest,
  deleteGithubSource,
  type GithubSourceView,
  githubSourceQueryKey,
  githubSourceQueryOptions,
  type Platform,
  putGithubSource,
} from "@/lib/api/github-source";
import { usePermissions } from "@/lib/query/usePermissions";

export const Route = createFileRoute("/_auth/apps/$slug/source")({
  component: SourceTab,
});

interface FormValues {
  owner: string;
  repo: string;
  tag_template?: string;
  access_token?: string;
  enabled: boolean;
  prefer_for_platforms?: Platform[];
}

const PLATFORM_LABELS: Record<Platform, string> = {
  "react-native-android": "React Native Android (APK)",
  "tauri-desktop": "Tauri Desktop",
};

function SourceTab() {
  const { t } = useLingui();
  const { notification, modal } = App.useApp();
  const queryClient = useQueryClient();
  const { has } = usePermissions();
  const { slug } = Route.useParams();

  const canManage = has("app:update");
  const sourceQuery = useQuery(githubSourceQueryOptions(slug));
  const source = sourceQuery.data ?? null;

  const [drawerOpen, setDrawerOpen] = useState(false);

  const invalidate = () => queryClient.invalidateQueries({ queryKey: githubSourceQueryKey(slug) });

  async function handleSubmit(values: FormValues): Promise<boolean> {
    try {
      const body: CreateGithubSourceRequest = {
        owner: values.owner.trim(),
        repo: values.repo.trim(),
        tag_template: values.tag_template?.trim() || null,
        enabled: values.enabled,
        // 表单每次都提交完整勾选集(含空数组 = 全部优先 OSS)。服务端「缺省即保留」的三态
        // 是给 CLI / 部分更新用的；表单是全量编辑，不传才会让「取消所有勾选」保存不下去。
        prefer_for_platforms: values.prefer_for_platforms ?? [],
      };
      // 留空 = 不改：仅在填了 token 时才带上，避免用空串覆盖已存令牌。
      if (values.access_token) {
        body.access_token = values.access_token;
      }
      await putGithubSource(slug, body);
      notification.success({ message: source ? t`来源已更新` : t`来源已配置` });
      setDrawerOpen(false);
      await invalidate();
      return true;
    } catch (error) {
      notification.error({
        message: t`保存失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
      return false;
    }
  }

  function handleDelete() {
    modal.confirm({
      title: t`移除 GitHub 来源？`,
      content: t`移除后该应用的产物将不再从 GitHub Release 分发；已发布产物上已记录的 mirror 地址不受影响。`,
      okText: t`移除`,
      okButtonProps: { danger: true },
      cancelText: t`取消`,
      onOk: async () => {
        try {
          await deleteGithubSource(slug);
          notification.success({ message: t`已移除` });
          await invalidate();
        } catch (error) {
          notification.error({
            message: t`移除失败`,
            description: isApiError(error) ? error.detail : String(error),
          });
        }
      },
    });
  }

  return (
    <Card
      title={t`GitHub Release 来源`}
      size="small"
      loading={sourceQuery.isPending}
      extra={
        canManage ? (
          <Space size="small">
            <Button
              type="primary"
              size="small"
              icon={source ? <EditOutlined /> : <PlusOutlined />}
              onClick={() => setDrawerOpen(true)}
            >
              {source ? <Trans>编辑</Trans> : <Trans>配置来源</Trans>}
            </Button>
            {source ? (
              <Button size="small" danger onClick={handleDelete}>
                <Trans>移除</Trans>
              </Button>
            ) : null}
          </Space>
        ) : null
      }
    >
      <Space direction="vertical" size="middle" style={{ width: "100%" }}>
        {source ? (
          <Descriptions column={1} size="small">
            <Descriptions.Item label={t`仓库`}>
              <Typography.Text copyable>
                {source.owner}/{source.repo}
              </Typography.Text>
            </Descriptions.Item>
            <Descriptions.Item label={t`Tag 模板`}>
              <Typography.Text code>{source.tag_template}</Typography.Text>
            </Descriptions.Item>
            <Descriptions.Item label={t`状态`}>
              {source.enabled ? (
                <Tag color="green">
                  <Trans>已启用</Trans>
                </Tag>
              ) : (
                <Tag>
                  <Trans>已停用</Trans>
                </Tag>
              )}
            </Descriptions.Item>
            <Descriptions.Item label={t`访问令牌`}>
              {source.token_set ? (
                <Tag color="blue">
                  <Trans>已设置</Trans>
                </Tag>
              ) : (
                <Tag>
                  <Trans>未设置</Trans>
                </Tag>
              )}
            </Descriptions.Item>
            <Descriptions.Item label={t`优先下载源`}>
              {/* 空态显式写成「全部平台优先对象存储」而非留白：留白会被读成「没配好 / 坏了」，
                  而它其实是一个明确且正确的状态（也是默认值）。 */}
              {source.prefer_for_platforms.length > 0 ? (
                <Space size={4} wrap>
                  {source.prefer_for_platforms.map((p) => (
                    <Tag color="purple" key={p}>
                      {PLATFORM_LABELS[p] ?? p} → GitHub
                    </Tag>
                  ))}
                </Space>
              ) : (
                <Typography.Text type="secondary">
                  <Trans>全部平台优先对象存储</Trans>
                </Typography.Text>
              )}
            </Descriptions.Item>
          </Descriptions>
        ) : (
          <Empty description={t`尚未配置 GitHub Release 来源`} />
        )}

        <Alert
          type="info"
          showIcon
          message={
            <Trans>
              配置后，可在注册产物时把 GitHub Release 资源地址作为镜像下载源；下载入口会在对象存储与
              GitHub 之间自动回退。Tag 模板用于探活时定位 Release（默认 v&#123;version&#125;）。
            </Trans>
          }
        />
      </Space>

      <SourceDrawer
        editing={source}
        open={drawerOpen}
        onOpenChange={setDrawerOpen}
        onFinish={handleSubmit}
      />
    </Card>
  );
}

function SourceDrawer({
  editing,
  open,
  onOpenChange,
  onFinish,
}: {
  editing: GithubSourceView | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onFinish: (v: FormValues) => Promise<boolean>;
}) {
  const { t } = useLingui();
  return (
    <DrawerForm<FormValues>
      // key remount：切换「配置 / 编辑」时让 initialValues 重新生效（admin-spa.md 坑）。
      key={editing?.id ?? "new"}
      title={editing ? t`编辑 GitHub 来源` : t`配置 GitHub 来源`}
      open={open}
      onOpenChange={onOpenChange}
      drawerProps={{ destroyOnClose: true }}
      initialValues={
        editing
          ? {
              owner: editing.owner,
              repo: editing.repo,
              tag_template: editing.tag_template,
              enabled: editing.enabled,
              prefer_for_platforms: editing.prefer_for_platforms,
            }
          : { enabled: true, prefer_for_platforms: [] }
      }
      onFinish={onFinish}
    >
      <ProFormText
        name="owner"
        label="Owner"
        tooltip={t`GitHub 仓库所有者（用户名或组织名）`}
        rules={[{ required: true, message: t`请输入 owner` }]}
      />
      <ProFormText
        name="repo"
        label="Repo"
        tooltip={t`仓库名`}
        rules={[{ required: true, message: t`请输入 repo` }]}
      />
      <ProFormText
        name="tag_template"
        label={t`Tag 模板`}
        placeholder="v{version}"
        tooltip={t`留空默认 v{version}；仅用于探活定位 Release，不参与下载地址拼接`}
      />
      <ProFormText.Password
        name="access_token"
        label={t`访问令牌（可选）`}
        placeholder={editing?.token_set ? t`留空表示不修改` : t`用于私有 / 限速仓库的探活`}
        tooltip={t`仅用于服务端探活，不会在任何响应中回传`}
      />
      <ProFormSwitch name="enabled" label={t`启用`} />
      <ProFormCheckbox.Group
        name="prefer_for_platforms"
        label={t`优先从 GitHub 下载的平台`}
        tooltip={t`勾选的平台，下载入口会先尝试 GitHub、失败再回退对象存储；未勾选的平台维持对象存储优先。按平台而非按应用勾选，可避免把桌面安装包也推去 GitHub。`}
        options={(Object.keys(PLATFORM_LABELS) as Platform[]).map((value) => ({
          value,
          label: PLATFORM_LABELS[value],
        }))}
        extra={
          <Trans>
            阿里云对象存储限制匿名下载 APK（返回 XML 错误页而非安装包），此时应勾选 React Native
            Android。桌面安装包不受该限制，且国内从对象存储下载更快，通常不必勾选。
          </Trans>
        }
      />
    </DrawerForm>
  );
}
