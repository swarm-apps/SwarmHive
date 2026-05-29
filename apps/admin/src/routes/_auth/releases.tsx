import { PlusOutlined } from "@ant-design/icons";
import {
  DrawerForm,
  ModalForm,
  PageContainer,
  type ProColumns,
  ProFormDigit,
  ProFormSelect,
  ProFormText,
  ProFormTextArea,
  ProTable,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import {
  App,
  Button,
  Card,
  Descriptions,
  Drawer,
  Empty,
  List,
  Popconfirm,
  Select,
  Space,
  Tag,
} from "antd";
import dayjs from "dayjs";
import { useState } from "react";
import { z } from "zod";
import { isApiError } from "@/lib/api";
import { appsQueryOptions, channelsQueryOptions, platformLabel } from "@/lib/api/apps";
import { ERR_CONFLICT, ERR_NOTHING_TO_ROLLBACK } from "@/lib/api/errors";
import {
  artifactsQueryOptions,
  canPublish,
  canYank,
  channelReleaseQueryOptions,
  createRelease,
  promote,
  publishedVersions,
  publishRelease,
  type Release,
  type ReleaseStatus,
  releaseStatusColor,
  releasesQueryOptions,
  rollback,
  updateRelease,
  yankRelease,
} from "@/lib/api/releases";
import { usePermissions } from "@/lib/query/usePermissions";

const searchSchema = z.object({ app: z.string().optional() });

export const Route = createFileRoute("/_auth/releases")({
  validateSearch: searchSchema,
  component: ReleasesPage,
});

interface CreateReleaseValues {
  version: string;
  android_version_code?: number;
  release_notes?: string;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = n;
  let i = -1;
  do {
    value /= 1024;
    i += 1;
  } while (value >= 1024 && i < units.length - 1);
  return `${value.toFixed(1)} ${units[i]}`;
}

function ReleaseStatusTag({ status }: { status: ReleaseStatus }) {
  const color = releaseStatusColor(status);
  switch (status) {
    case "draft":
      return (
        <Tag color={color}>
          <Trans>草稿</Trans>
        </Tag>
      );
    case "published":
      return (
        <Tag color={color}>
          <Trans>已发布</Trans>
        </Tag>
      );
    case "yanked":
      return (
        <Tag color={color}>
          <Trans>已撤回</Trans>
        </Tag>
      );
    default:
      return <Tag>{status}</Tag>;
  }
}

function ReleasesPage() {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();
  const { has } = usePermissions();
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const slug = search.app ?? "";

  const appsQuery = useQuery(appsQueryOptions());
  const releasesQuery = useQuery(releasesQueryOptions(slug));

  const [editing, setEditing] = useState<Release | null>(null);
  const [artifactsVersion, setArtifactsVersion] = useState<string | null>(null);

  const canCreate = has("release:create");
  const canUpdate = has("release:update");
  const canPub = has("release:publish");
  const canYankAction = has("release:yank");

  const apps = appsQuery.data ?? [];
  const releases = releasesQuery.data ?? [];

  const invalidateReleases = () =>
    queryClient.invalidateQueries({ queryKey: releasesQueryOptions(slug).queryKey });

  function selectApp(next: string | undefined) {
    navigate({ search: { app: next || undefined } });
  }

  async function handleCreate(values: CreateReleaseValues): Promise<boolean> {
    try {
      await createRelease(slug, {
        version: values.version.trim(),
        android_version_code: values.android_version_code ?? null,
        release_notes: values.release_notes?.trim() || null,
      });
      notification.success({ message: t`版本已创建` });
      await invalidateReleases();
      return true;
    } catch (error) {
      if (isApiError(error) && error.type === ERR_CONFLICT) {
        notification.error({ message: t`该版本号已存在` });
        return false;
      }
      notification.error({ message: t`创建失败，请稍后重试` });
      return false;
    }
  }

  async function handleEdit(values: CreateReleaseValues): Promise<boolean> {
    if (!editing) return false;
    try {
      await updateRelease(slug, editing.version, {
        android_version_code: values.android_version_code ?? null,
        release_notes: values.release_notes?.trim() || null,
      });
      notification.success({ message: t`版本已更新` });
      await invalidateReleases();
      setEditing(null);
      return true;
    } catch {
      notification.error({ message: t`更新失败，请稍后重试` });
      return false;
    }
  }

  async function handlePublish(release: Release): Promise<void> {
    try {
      await publishRelease(slug, release.version);
      notification.success({ message: t`版本已发布` });
      await invalidateReleases();
    } catch {
      notification.error({ message: t`发布失败，请稍后重试` });
    }
  }

  async function handleYank(release: Release): Promise<void> {
    try {
      await yankRelease(slug, release.version);
      notification.success({ message: t`版本已撤回` });
      await invalidateReleases();
    } catch {
      notification.error({ message: t`撤回失败，请稍后重试` });
    }
  }

  const columns: ProColumns<Release>[] = [
    { title: t`版本号`, dataIndex: "version", copyable: true },
    {
      title: t`Android versionCode`,
      dataIndex: "android_version_code",
      width: 170,
      render: (_, row) => row.android_version_code ?? "-",
    },
    {
      title: t`状态`,
      dataIndex: "status",
      width: 110,
      render: (_, row) => <ReleaseStatusTag status={row.status} />,
    },
    {
      title: t`发布时间`,
      dataIndex: "published_at",
      width: 180,
      render: (_, row) =>
        row.published_at ? dayjs(row.published_at).format("YYYY-MM-DD HH:mm") : "-",
    },
    {
      title: t`创建时间`,
      dataIndex: "created_at",
      width: 180,
      render: (_, row) => dayjs(row.created_at).format("YYYY-MM-DD HH:mm"),
    },
    {
      title: t`操作`,
      width: 240,
      render: (_, row) => (
        <Space size="small">
          <Button type="link" size="small" onClick={() => setArtifactsVersion(row.version)}>
            <Trans>产物</Trans>
          </Button>
          {canUpdate && (
            <Button type="link" size="small" onClick={() => setEditing(row)}>
              <Trans>编辑</Trans>
            </Button>
          )}
          {canPub && canPublish(row.status) && (
            <Popconfirm
              title={t`确认发布该版本？`}
              onConfirm={() => handlePublish(row)}
              okText={t`发布`}
              cancelText={t`取消`}
            >
              <Button type="link" size="small">
                <Trans>发布</Trans>
              </Button>
            </Popconfirm>
          )}
          {canYankAction && canYank(row.status) && (
            <Popconfirm
              title={t`确认撤回该版本？`}
              onConfirm={() => handleYank(row)}
              okText={t`撤回`}
              cancelText={t`取消`}
            >
              <Button type="link" size="small" danger>
                <Trans>撤回</Trans>
              </Button>
            </Popconfirm>
          )}
        </Space>
      ),
    },
  ];

  return (
    <PageContainer title={t`版本`} breadcrumbRender={false}>
      {apps.length === 0 ? (
        <Empty description={t`还没有应用，请先创建应用`}>
          <Link to="/apps">
            <Button type="primary">
              <Trans>去创建应用</Trans>
            </Button>
          </Link>
        </Empty>
      ) : (
        <Space direction="vertical" style={{ width: "100%" }} size="large">
          <Select
            style={{ width: 320 }}
            placeholder={t`选择应用`}
            value={slug || undefined}
            onChange={selectApp}
            showSearch
            optionFilterProp="label"
            options={apps.map((a) => ({ label: `${a.display_name} (${a.slug})`, value: a.slug }))}
          />

          {slug ? (
            <>
              <ProTable<Release>
                rowKey="id"
                search={false}
                options={false}
                loading={releasesQuery.isPending}
                dataSource={releases}
                pagination={{ pageSize: 20, hideOnSinglePage: true }}
                locale={{ emptyText: <Empty description={t`该应用还没有版本`} /> }}
                toolBarRender={() =>
                  canCreate ? [<CreateReleaseDrawer key="create" onFinish={handleCreate} />] : []
                }
                columns={columns}
              />

              <ReleaseTrainPanel slug={slug} published={publishedVersions(releases)} has={has} />
            </>
          ) : (
            <Empty description={t`请选择一个应用查看其版本`} />
          )}
        </Space>
      )}

      <EditReleaseDrawer
        editing={editing}
        onOpenChange={(o) => !o && setEditing(null)}
        onFinish={handleEdit}
      />

      <ArtifactsDrawer
        slug={slug}
        version={artifactsVersion}
        onClose={() => setArtifactsVersion(null)}
      />
    </PageContainer>
  );
}

function CreateReleaseDrawer({
  onFinish,
}: {
  onFinish: (v: CreateReleaseValues) => Promise<boolean>;
}) {
  const { t } = useLingui();

  return (
    <DrawerForm<CreateReleaseValues>
      title={t`创建版本`}
      trigger={
        <Button type="primary" icon={<PlusOutlined />}>
          <Trans>创建版本</Trans>
        </Button>
      }
      drawerProps={{ destroyOnClose: true }}
      onFinish={onFinish}
    >
      <ProFormText
        name="version"
        label={t`版本号`}
        tooltip={t`应用内唯一，发布后作为下载寻址`}
        rules={[{ required: true, message: t`请输入版本号` }]}
      />
      <ProFormDigit
        name="android_version_code"
        label={t`Android versionCode`}
        tooltip={t`React Native Android 用于比较的单调整数；Tauri 可留空`}
        min={1}
        fieldProps={{ precision: 0 }}
      />
      <ProFormTextArea name="release_notes" label={t`发布说明`} fieldProps={{ rows: 4 }} />
    </DrawerForm>
  );
}

function EditReleaseDrawer({
  editing,
  onOpenChange,
  onFinish,
}: {
  editing: Release | null;
  onOpenChange: (open: boolean) => void;
  onFinish: (v: CreateReleaseValues) => Promise<boolean>;
}) {
  const { t } = useLingui();

  return (
    <DrawerForm<CreateReleaseValues>
      key={editing?.version ?? "none"}
      title={t`编辑版本`}
      open={editing != null}
      onOpenChange={onOpenChange}
      drawerProps={{ destroyOnClose: true }}
      initialValues={
        editing
          ? {
              android_version_code: editing.android_version_code ?? undefined,
              release_notes: editing.release_notes ?? undefined,
            }
          : undefined
      }
      onFinish={onFinish}
    >
      <ProFormText label={t`版本号`} fieldProps={{ value: editing?.version, disabled: true }} />
      <ProFormDigit
        name="android_version_code"
        label={t`Android versionCode`}
        min={1}
        fieldProps={{ precision: 0 }}
      />
      <ProFormTextArea name="release_notes" label={t`发布说明`} fieldProps={{ rows: 4 }} />
    </DrawerForm>
  );
}

function ArtifactsDrawer({
  slug,
  version,
  onClose,
}: {
  slug: string;
  version: string | null;
  onClose: () => void;
}) {
  const { t } = useLingui();
  const artifactsQuery = useQuery(artifactsQueryOptions(slug, version ?? ""));

  return (
    <Drawer
      title={version ? t`产物 · ${version}` : t`产物`}
      open={version != null}
      onClose={onClose}
      width={560}
      destroyOnClose
    >
      <List
        loading={artifactsQuery.isPending}
        dataSource={artifactsQuery.data ?? []}
        locale={{ emptyText: <Empty description={t`该版本暂无产物`} /> }}
        renderItem={(art) => (
          <List.Item>
            <Descriptions size="small" column={1} title={art.filename} style={{ width: "100%" }}>
              <Descriptions.Item label={t`平台`}>{platformLabel(art.platform)}</Descriptions.Item>
              {art.target && <Descriptions.Item label="target">{art.target}</Descriptions.Item>}
              {art.arch && <Descriptions.Item label="arch">{art.arch}</Descriptions.Item>}
              {art.abi && <Descriptions.Item label="abi">{art.abi}</Descriptions.Item>}
              <Descriptions.Item label={t`大小`}>{formatBytes(art.size_bytes)}</Descriptions.Item>
              <Descriptions.Item label="sha256">
                <span style={{ fontFamily: "monospace", wordBreak: "break-all" }}>
                  {art.sha256}
                </span>
              </Descriptions.Item>
            </Descriptions>
          </List.Item>
        )}
      />
    </Drawer>
  );
}

function ReleaseTrainPanel({
  slug,
  published,
  has,
}: {
  slug: string;
  published: Release[];
  has: (p: Parameters<ReturnType<typeof usePermissions>["has"]>[0]) => boolean;
}) {
  const { t } = useLingui();
  const channelsQuery = useQuery(channelsQueryOptions(slug));
  const channels = channelsQuery.data ?? [];

  return (
    <Card title={t`发布列车`} size="small" loading={channelsQuery.isPending}>
      <List
        dataSource={channels}
        locale={{ emptyText: <Empty description={t`暂无 Channel`} /> }}
        renderItem={(ch) => (
          <ChannelPointerRow
            slug={slug}
            channel={ch.name}
            isDefault={ch.is_default}
            published={published}
            canPromote={has("release:promote")}
            canRollback={has("release:rollback")}
          />
        )}
      />
    </Card>
  );
}

function ChannelPointerRow({
  slug,
  channel,
  isDefault,
  published,
  canPromote,
  canRollback,
}: {
  slug: string;
  channel: string;
  isDefault: boolean;
  published: Release[];
  canPromote: boolean;
  canRollback: boolean;
}) {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();
  const pointerQuery = useQuery(channelReleaseQueryOptions(slug, channel));
  const current = pointerQuery.data ?? null;

  const invalidatePointer = () =>
    queryClient.invalidateQueries({ queryKey: channelReleaseQueryOptions(slug, channel).queryKey });

  async function handlePromote(values: { version: string }): Promise<boolean> {
    try {
      await promote(slug, channel, { version: values.version });
      notification.success({ message: t`已 promote 到 ${channel}` });
      await invalidatePointer();
      return true;
    } catch (error) {
      if (isApiError(error) && error.type === ERR_CONFLICT) {
        notification.error({ message: t`只能 promote 已发布的版本` });
        return false;
      }
      notification.error({ message: t`promote 失败，请稍后重试` });
      return false;
    }
  }

  async function handleRollback(): Promise<void> {
    try {
      await rollback(slug, channel, {});
      notification.success({ message: t`已回滚 ${channel}` });
      await invalidatePointer();
    } catch (error) {
      if (isApiError(error) && error.type === ERR_NOTHING_TO_ROLLBACK) {
        notification.error({ message: t`无可回滚版本` });
        return;
      }
      notification.error({ message: t`回滚失败，请稍后重试` });
    }
  }

  return (
    <List.Item
      actions={[
        ...(canPromote
          ? [
              <ModalForm<{ version: string }>
                key="promote"
                title={t`promote 到 ${channel}`}
                trigger={
                  <Button type="link" size="small">
                    <Trans>promote</Trans>
                  </Button>
                }
                modalProps={{ destroyOnClose: true }}
                width={360}
                onFinish={handlePromote}
              >
                <ProFormSelect
                  name="version"
                  label={t`已发布版本`}
                  options={published.map((r) => ({ label: r.version, value: r.version }))}
                  rules={[{ required: true, message: t`请选择版本` }]}
                />
              </ModalForm>,
            ]
          : []),
        ...(canRollback
          ? [
              <Popconfirm
                key="rollback"
                title={t`回滚 ${channel} 到上一个版本？`}
                onConfirm={handleRollback}
                okText={t`回滚`}
                cancelText={t`取消`}
              >
                <Button type="link" size="small" danger>
                  <Trans>回滚</Trans>
                </Button>
              </Popconfirm>,
            ]
          : []),
      ]}
    >
      <Space>
        <strong>{channel}</strong>
        {isDefault && (
          <Tag color="green">
            <Trans>默认</Trans>
          </Tag>
        )}
        <span style={{ color: "rgba(0,0,0,0.45)" }}>
          {pointerQuery.isPending ? "…" : (current?.version ?? t`未指向版本`)}
        </span>
      </Space>
    </List.Item>
  );
}
