import { UploadOutlined } from "@ant-design/icons";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, redirect } from "@tanstack/react-router";
import { App, Button, Card, Descriptions, Modal, Popconfirm, Space, Typography } from "antd";
import dayjs from "dayjs";
import { useState } from "react";
import { isApiError } from "@/lib/api";
import {
  canPublish,
  canYank,
  publishRelease,
  type Release,
  releasesQueryOptions,
  updateRelease,
  yankRelease,
} from "@/lib/api/releases";
import { usePermissions } from "@/lib/query/usePermissions";
import {
  ArtifactsTable,
  EditReleaseDrawer,
  type EditReleaseValues,
  policyUpdateFields,
  ReleaseStatusTag,
  UploadArtifacts,
} from "./-shared";

export const Route = createFileRoute("/_auth/apps/$slug/releases/$version")({
  // 详情页：先确认 release 存在（复用版本列表 query，零后端新增），404 兜底回列表。
  beforeLoad: async ({ context, params }) => {
    const releases = await context.queryClient.ensureQueryData(releasesQueryOptions(params.slug));
    if (!releases.some((r) => r.version === params.version)) {
      throw redirect({
        to: "/apps/$slug/releases",
        params: { slug: params.slug },
        replace: true,
      });
    }
  },
  component: ReleaseDetail,
});

function ReleaseDetail() {
  const { t } = useLingui();
  const { slug, version } = Route.useParams();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();
  const { has } = usePermissions();

  const releasesQuery = useQuery(releasesQueryOptions(slug));
  const release = releasesQuery.data?.find((r) => r.version === version);

  const [editing, setEditing] = useState<Release | null>(null);
  const [uploadOpen, setUploadOpen] = useState(false);

  const canUpdate = has("release:update");
  const canPub = has("release:publish");
  const canYankAction = has("release:yank");
  const canUpload = has("artifact:upload");

  const invalidateReleases = () =>
    queryClient.invalidateQueries({ queryKey: releasesQueryOptions(slug).queryKey });

  async function handleEdit(values: EditReleaseValues): Promise<boolean> {
    if (!release) return false;
    try {
      await updateRelease(slug, release.version, {
        android_version_code: values.android_version_code ?? null,
        release_notes: values.release_notes?.trim() || null,
        ...policyUpdateFields(values, release),
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

  async function handlePublish(): Promise<void> {
    if (!release) return;
    try {
      await publishRelease(slug, release.version);
      notification.success({ message: t`版本已发布` });
      await invalidateReleases();
    } catch {
      notification.error({ message: t`发布失败，请稍后重试` });
    }
  }

  async function handleYank(): Promise<void> {
    if (!release) return;
    try {
      await yankRelease(slug, release.version);
      notification.success({ message: t`版本已撤回` });
      await invalidateReleases();
    } catch {
      notification.error({ message: t`撤回失败，请稍后重试` });
    }
  }

  // beforeLoad 已保证 release 存在；加载态短暂返回 null。
  if (!release) return null;

  return (
    <Space direction="vertical" size="middle" style={{ width: "100%" }}>
      <Card>
        <Descriptions
          column={1}
          title={
            <Space size="small">
              <Typography.Text copyable>{release.version}</Typography.Text>
              <ReleaseStatusTag status={release.status} />
            </Space>
          }
          extra={
            <Space>
              {canUpload && (
                <Button
                  type="primary"
                  icon={<UploadOutlined />}
                  onClick={() => setUploadOpen(true)}
                >
                  <Trans>上传产物</Trans>
                </Button>
              )}
              {canUpdate && (
                <Button onClick={() => setEditing(release)}>
                  <Trans>编辑</Trans>
                </Button>
              )}
              {canPub && canPublish(release.status) && (
                <Popconfirm
                  title={t`确认发布该版本？`}
                  onConfirm={handlePublish}
                  okText={t`发布`}
                  cancelText={t`取消`}
                >
                  <Button>
                    <Trans>发布</Trans>
                  </Button>
                </Popconfirm>
              )}
              {canYankAction && canYank(release.status) && (
                <Popconfirm
                  title={t`确认撤回该版本？`}
                  onConfirm={handleYank}
                  okText={t`撤回`}
                  cancelText={t`取消`}
                >
                  <Button danger>
                    <Trans>撤回</Trans>
                  </Button>
                </Popconfirm>
              )}
            </Space>
          }
        >
          {release.android_version_code != null && (
            <Descriptions.Item label="Android versionCode">
              {release.android_version_code}
            </Descriptions.Item>
          )}
          <Descriptions.Item label={t`灰度放量`}>
            {release.rollout_percent != null && release.rollout_percent < 100
              ? `${release.rollout_percent}%`
              : t`100%（全量）`}
          </Descriptions.Item>
          <Descriptions.Item label={t`强更下限`}>
            {[
              release.min_version && release.min_version !== "0.0.0"
                ? `Tauri ≥ ${release.min_version}`
                : null,
              release.android_min_version_code != null
                ? `Android ≥ ${release.android_min_version_code}`
                : null,
            ]
              .filter(Boolean)
              .join(" · ") || t`无`}
          </Descriptions.Item>
          <Descriptions.Item label={t`发布时间`}>
            {release.published_at ? dayjs(release.published_at).format("YYYY-MM-DD HH:mm") : "-"}
          </Descriptions.Item>
          <Descriptions.Item label={t`创建时间`}>
            {dayjs(release.created_at).format("YYYY-MM-DD HH:mm")}
          </Descriptions.Item>
          <Descriptions.Item label={t`发布说明`}>
            {release.release_notes ? (
              <Typography.Paragraph style={{ marginBottom: 0, whiteSpace: "pre-wrap" }}>
                {release.release_notes}
              </Typography.Paragraph>
            ) : (
              "-"
            )}
          </Descriptions.Item>
        </Descriptions>
      </Card>

      <Card title={t`产物`} styles={{ body: { paddingBlock: 0 } }}>
        <ArtifactsTable slug={slug} version={version} />
      </Card>

      <Modal
        title={t`上传产物 · ${version}`}
        open={uploadOpen}
        onCancel={() => setUploadOpen(false)}
        width={780}
        footer={null}
        destroyOnClose
      >
        <UploadArtifacts slug={slug} version={version} />
      </Modal>

      <EditReleaseDrawer
        editing={editing}
        onOpenChange={(o) => !o && setEditing(null)}
        onFinish={handleEdit}
      />
    </Space>
  );
}
