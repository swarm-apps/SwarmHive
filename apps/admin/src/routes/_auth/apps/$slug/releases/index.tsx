import { type ProColumns, ProTable } from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { App, Button, Empty, Popconfirm, Space } from "antd";
import dayjs from "dayjs";
import { useState } from "react";
import { isApiError } from "@/lib/api";
import { ERR_CONFLICT } from "@/lib/api/errors";
import {
  canPublish,
  canYank,
  createRelease,
  publishRelease,
  type Release,
  releasesQueryOptions,
  updateRelease,
  yankRelease,
} from "@/lib/api/releases";
import { usePermissions } from "@/lib/query/usePermissions";
import {
  CreateReleaseDrawer,
  type CreateReleaseValues,
  EditReleaseDrawer,
  type EditReleaseValues,
  policyUpdateFields,
  ReleaseStatusTag,
} from "./-shared";

export const Route = createFileRoute("/_auth/apps/$slug/releases/")({
  component: ReleasesTab,
});

function ReleasesTab() {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();
  const { has } = usePermissions();
  const { slug } = Route.useParams();
  const navigate = Route.useNavigate();

  const releasesQuery = useQuery(releasesQueryOptions(slug));

  const [editing, setEditing] = useState<Release | null>(null);

  const canCreate = has("release:create");
  const canUpdate = has("release:update");
  const canPub = has("release:publish");
  const canYankAction = has("release:yank");

  const releases = releasesQuery.data ?? [];

  const invalidateReleases = () =>
    queryClient.invalidateQueries({ queryKey: releasesQueryOptions(slug).queryKey });

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

  async function handleEdit(values: EditReleaseValues): Promise<boolean> {
    if (!editing) return false;
    try {
      await updateRelease(slug, editing.version, {
        android_version_code: values.android_version_code ?? null,
        release_notes: values.release_notes?.trim() || null,
        ...policyUpdateFields(values, editing),
      });
      notification.success({ message: t`版本已更新` });
      await invalidateReleases();
      setEditing(null);
      return true;
    } catch (error) {
      notification.error({
        message: isApiError(error) ? error.detail : t`更新失败，请稍后重试`,
      });
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
          <Button
            type="link"
            size="small"
            onClick={() =>
              navigate({
                to: "/apps/$slug/releases/$version",
                params: { slug, version: row.version },
              })
            }
          >
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

      <EditReleaseDrawer
        editing={editing}
        onOpenChange={(o) => !o && setEditing(null)}
        onFinish={handleEdit}
      />
    </>
  );
}
