import { InboxOutlined, PlusOutlined, UploadOutlined } from "@ant-design/icons";
import {
  DrawerForm,
  type ProColumns,
  ProForm,
  ProFormDependency,
  ProFormDigit,
  ProFormSelect,
  ProFormText,
  ProFormTextArea,
  ProTable,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Alert,
  App,
  Button,
  Card,
  Checkbox,
  Descriptions,
  Empty,
  Input,
  Progress,
  Segmented,
  Select,
  Space,
  Table,
  type TableColumnsType,
  Tag,
  Typography,
  Upload,
} from "antd";
import dayjs from "dayjs";
import { useState } from "react";
import { isApiError } from "@/lib/api";
import { channelsQueryOptions, PLATFORMS, type Platform, platformLabel } from "@/lib/api/apps";
import { ERR_UPLOAD_CHECKSUM_MISMATCH } from "@/lib/api/errors";
import {
  type Artifact,
  type ArtifactKind,
  artifactsQueryOptions,
  channelReleaseQueryOptions,
  promote,
  type Release,
  type ReleaseStatus,
  releaseStatusColor,
  releasesQueryOptions,
} from "@/lib/api/releases";
import {
  type CompletePart,
  completeUpload,
  type PresignFile,
  presignUpload,
  putToStorage,
} from "@/lib/api/uploads";
import { usePermissions } from "@/lib/query/usePermissions";
import { friendlyArch, platformRowSpans } from "@/lib/upload/artifact-display";
import { classifyArtifact, pairSignatures } from "@/lib/upload/classify";
import { hashFile } from "@/lib/upload/hash";

export interface CreateReleaseValues {
  version: string;
  android_version_code?: number;
  release_notes?: string;
}

/** 编辑抽屉额外暴露灰度 / 强更策略(创建时不设,故只在编辑用)。 */
export interface EditReleaseValues extends CreateReleaseValues {
  /** 灰度放量 1-100;100=全量。 */
  rollout_percent?: number;
  /** Tauri 强更下限(semver);空=不改,填 0.0.0 移除下限。 */
  min_version?: string;
  /** RN Android 强更下限(versionCode);空=不改。 */
  android_min_version_code?: number;
}

/**
 * 把编辑表单值映射成 `UpdateReleaseRequest` 的策略字段,统一两处 handler(列表 / 详情)。
 * 后端是单层 Option「null=不改、清空走 sentinel」,这里对比**初值**实现直觉化清空:
 * - `min_version`:非空→该值;留空且原本有下限→`"0.0.0"`(移除下限);留空且原本无下限→`null`(不改)。
 * - `rollout_percent`:<100→设灰度;原本有灰度而现在填回 100→`100`(取消灰度);原本无灰度且 100→`null`
 *   (不改——避免把 NULL 漂移成显式 100)。
 */
export function policyUpdateFields(values: EditReleaseValues, editing: Release) {
  const hadMinFloor = !!editing.min_version && editing.min_version !== "0.0.0";
  const hadRollout = editing.rollout_percent != null && editing.rollout_percent < 100;
  const minInput = values.min_version?.trim();
  const rollout = values.rollout_percent;
  return {
    min_version: minInput || (hadMinFloor ? "0.0.0" : null),
    rollout_percent: rollout != null && rollout < 100 ? rollout : hadRollout ? 100 : null,
    android_min_version_code: values.android_min_version_code ?? null,
  };
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

export function ReleaseStatusTag({ status }: { status: ReleaseStatus }) {
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

export function CreateReleaseDrawer({
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

export function EditReleaseDrawer({
  editing,
  onOpenChange,
  onFinish,
}: {
  editing: Release | null;
  onOpenChange: (open: boolean) => void;
  onFinish: (v: EditReleaseValues) => Promise<boolean>;
}) {
  const { t } = useLingui();

  return (
    <DrawerForm<EditReleaseValues>
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
              rollout_percent: editing.rollout_percent ?? 100,
              min_version: editing.min_version ?? undefined,
              android_min_version_code: editing.android_min_version_code ?? undefined,
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
      <ProFormDigit
        name="rollout_percent"
        label={t`灰度放量 (%)`}
        tooltip={t`1-100；100=全量发布，<100 按 client_id 哈希分桶灰度（SDK 需传 client_id）`}
        min={1}
        max={100}
        fieldProps={{ precision: 0 }}
        rules={[{ type: "number", min: 1, max: 100, message: t`灰度须在 1-100 之间` }]}
      />
      <ProFormText
        name="min_version"
        label={t`强更下限 (Tauri semver)`}
        tooltip={t`低于此版本的客户端被强制更新；留空=不改，清空已设下限即移除（或填 0.0.0）`}
        placeholder={t`如 1.2.0`}
        rules={[
          {
            validator: (_, value: string) =>
              !value?.trim() || /^v?\d+\.\d+\.\d+([-+].*)?$/.test(value.trim())
                ? Promise.resolve()
                : Promise.reject(new Error(t`请输入合法 semver（如 1.2.0）`)),
          },
        ]}
      />
      <ProFormDigit
        name="android_min_version_code"
        label={t`强更下限 (Android versionCode)`}
        tooltip={t`RN Android：低于此 versionCode 的客户端被强制更新；留空=不改，调高即 kill switch`}
        min={1}
        fieldProps={{ precision: 0 }}
      />
      <ProFormTextArea name="release_notes" label={t`发布说明`} fieldProps={{ rows: 4 }} />
    </DrawerForm>
  );
}

export function ArtifactsTable({ slug, version }: { slug: string; version: string }) {
  const { t } = useLingui();
  const artifactsQuery = useQuery(artifactsQueryOptions(slug, version));
  const artifacts = artifactsQuery.data ?? [];
  // 先按 platform 顺序排好(PLATFORMS 顺序),再算每行 rowSpan 合并首列。
  const sorted = PLATFORMS.flatMap((p) => artifacts.filter((a) => a.platform === p));
  const spans = platformRowSpans(sorted.map((a) => a.platform));

  const columns: ProColumns<Artifact>[] = [
    {
      title: t`平台`,
      dataIndex: "platform",
      width: 96,
      render: (_, r) => platformLabel(r.platform),
      onCell: (_, index) => ({ rowSpan: spans[index ?? 0] ?? 0 }),
    },
    {
      title: t`架构`,
      width: 168,
      render: (_, r) => <Tag>{friendlyArch(r.platform, r.target, r.abi)}</Tag>,
    },
    {
      title: t`类型`,
      dataIndex: "kind",
      width: 96,
      render: (_, r) => <Tag>{r.kind}</Tag>,
    },
    { title: t`文件`, dataIndex: "filename", ellipsis: true },
    {
      title: t`大小`,
      dataIndex: "size_bytes",
      width: 96,
      align: "right",
      render: (_, r) => (
        <span style={{ fontVariantNumeric: "tabular-nums" }}>{formatBytes(r.size_bytes)}</span>
      ),
    },
    {
      // sha256 不用列级 ellipsis+copyable(与 render 同设会失效,pro-components #3872),
      // 改在 render 里用 Typography.Text 自带 copyable + ellipsis.tooltip。
      title: "sha256",
      width: 132,
      render: (_, r) => (
        <Typography.Text
          copyable={{ text: r.sha256 }}
          ellipsis={{ tooltip: r.sha256 }}
          style={{ maxWidth: 108, fontFamily: "monospace" }}
        >
          {r.sha256}
        </Typography.Text>
      ),
    },
    {
      title: t`签名`,
      width: 72,
      render: (_, r) =>
        r.signature_metadata != null ? (
          <Tag color="success">
            <Trans>已签</Trans>
          </Tag>
        ) : (
          <Tag>
            <Trans>未签</Trans>
          </Tag>
        ),
    },
    {
      title: t`操作`,
      width: 64,
      render: (_, r) => (
        <Button
          type="link"
          size="small"
          href={`/download/${slug}/${version}/${r.id}`}
          target="_blank"
          rel="noreferrer"
        >
          <Trans>下载</Trans>
        </Button>
      ),
    },
  ];

  return (
    <ProTable<Artifact>
      rowKey="id"
      search={false}
      options={false}
      loading={artifactsQuery.isPending}
      dataSource={sorted}
      pagination={false}
      locale={{ emptyText: <Empty description={t`该版本暂无产物`} /> }}
      columns={columns}
      expandable={{
        expandedRowRender: (r) => (
          <Descriptions size="small" column={1} style={{ paddingInlineStart: 8 }}>
            <Descriptions.Item label="sha256">
              <Typography.Text copyable style={{ fontFamily: "monospace", wordBreak: "break-all" }}>
                {r.sha256}
              </Typography.Text>
            </Descriptions.Item>
            {r.signature_metadata != null && (
              <Descriptions.Item label={t`签名`}>
                <Typography.Text
                  copyable={{ text: JSON.stringify(r.signature_metadata) }}
                  style={{ fontFamily: "monospace", wordBreak: "break-all" }}
                >
                  {JSON.stringify(r.signature_metadata)}
                </Typography.Text>
              </Descriptions.Item>
            )}
            <Descriptions.Item label={t`上传时间`}>
              {dayjs(r.created_at).format("YYYY-MM-DD HH:mm")}
            </Descriptions.Item>
          </Descriptions>
        ),
      }}
    />
  );
}

// ────────────────────────── 浏览器直传上传区 ──────────────────────────

type UploadStatus = "pending" | "hashing" | "uploading" | "done" | "error";

interface StagedItem {
  uid: string;
  file: File;
  platform: Platform;
  kind: ArtifactKind;
  target?: string;
  abi?: string;
  /** 未知扩展名:默认 tauri-desktop,需用户确认。 */
  uncertain: boolean;
  /** 配对的 .sig 文件名(展示用)。 */
  signatureName?: string;
  /** .sig 文本内容(complete 时随该 part 上送)。 */
  signatureText?: string;
  status: UploadStatus;
  hashRatio: number;
  uploadRatio: number;
  md5?: string;
  sha256?: string;
  error?: string;
}

// 引导式上传：平台 → 架构 候选。
const TAURI_TARGETS = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-pc-windows-msvc",
  "aarch64-pc-windows-msvc",
  "x86_64-unknown-linux-gnu",
  "aarch64-unknown-linux-gnu",
];
const ANDROID_ABIS = ["arm64-v8a", "armeabi-v7a", "x86_64", "x86"];

interface GuidedValues {
  platform: Platform;
  target?: string;
  abi?: string;
}

export function UploadArtifacts({ slug, version }: { slug: string; version: string }) {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const { has } = usePermissions();
  const queryClient = useQueryClient();
  const channelsQuery = useQuery(channelsQueryOptions(slug));

  const [mode, setMode] = useState<"guided" | "batch">("guided");
  const [items, setItems] = useState<StagedItem[]>([]);
  // 引导式的受控文件（不进 ProForm 字段）。
  const [guidedFile, setGuidedFile] = useState<File | null>(null);
  const [guidedSig, setGuidedSig] = useState<File | null>(null);
  const [publish, setPublish] = useState(true);
  const [promoteChannel, setPromoteChannel] = useState<string | undefined>(undefined);
  const [busy, setBusy] = useState(false);

  const canPromote = has("release:promote");

  function patch(uid: string, p: Partial<StagedItem>) {
    setItems((prev) => prev.map((it) => (it.uid === uid ? { ...it, ...p } : it)));
  }

  // 接收一批拖入 / 选择的文件:.sig 与同名 bundle 配对,孤立 .sig 报错不入栈。
  async function ingest(files: File[]) {
    const names = files.map((f) => f.name);
    const { bundles, signatureByBundle, orphanSignatures } = pairSignatures(names);
    if (orphanSignatures.length > 0) {
      notification.error({
        message: t`签名文件没有对应的产物`,
        description: orphanSignatures.join(", "),
      });
      return;
    }
    const byName = new Map(files.map((f) => [f.name, f]));
    const staged: StagedItem[] = [];
    for (const name of bundles) {
      const file = byName.get(name);
      if (!file) continue;
      const c = classifyArtifact(name);
      const sigName = signatureByBundle[name];
      const sigFile = sigName ? byName.get(sigName) : undefined;
      staged.push({
        uid: `${name}:${file.size}:${file.lastModified}`,
        file,
        platform: c.platform,
        kind: c.kind,
        target: c.target,
        abi: c.abi,
        uncertain: c.uncertain,
        signatureName: sigName,
        signatureText: sigFile ? (await sigFile.text()).trim() : undefined,
        status: "pending",
        hashRatio: 0,
        uploadRatio: 0,
      });
    }
    setItems((prev) => [...prev.filter((p) => !staged.some((s) => s.uid === p.uid)), ...staged]);
  }

  // 抽出的直传链路：hash → presign → 定长 PUT → complete → 可选 promote。引导式 + 批量共用。
  async function uploadItems(targets: StagedItem[]) {
    if (targets.length === 0) return;
    setBusy(true);
    try {
      // 1. 流式算 hash(Web Worker),逐文件进度。
      for (const w of targets) {
        if (w.md5 && w.sha256) continue;
        patch(w.uid, { status: "hashing", hashRatio: 0, error: undefined });
        const { md5, sha256 } = await hashFile(w.file, (r) => patch(w.uid, { hashRatio: r }));
        w.md5 = md5;
        w.sha256 = sha256;
        patch(w.uid, { md5, sha256, hashRatio: 1 });
      }
      // 2. presign(parts 顺序与 files 一致)。
      const files: PresignFile[] = targets.map((w) => ({
        relative_path: w.file.name,
        size: w.file.size,
        expected_sha256: w.sha256 ?? "",
        expected_md5: w.md5 ?? "",
        platform: w.platform,
        kind: w.kind,
        target: w.target || null,
        arch: null,
        abi: w.abi || null,
      }));
      const presign = await presignUpload(slug, version, files);
      // 3. 逐文件直传(XHR 进度)。
      for (let i = 0; i < targets.length; i++) {
        const w = targets[i];
        patch(w.uid, { status: "uploading", uploadRatio: 0 });
        await putToStorage(presign.parts[i], w.file, (r) => patch(w.uid, { uploadRatio: r }));
        patch(w.uid, { uploadRatio: 1 });
      }
      // 4. complete(可选发布);.sig 随对应 part 上送。
      const parts: CompletePart[] = presign.parts.map((p, i) => ({
        object_key: p.object_key,
        sha256: targets[i].sha256 ?? "",
        signature: targets[i].signatureText ?? null,
      }));
      await completeUpload(slug, version, presign.upload_id, parts, publish);
      for (const w of targets) patch(w.uid, { status: "done" });
      // 5. 可选 promote(需发布成功 + release:promote)。
      if (publish && promoteChannel && canPromote) {
        await promote(slug, promoteChannel, { version });
      }
      notification.success({ message: t`上传完成` });
      setItems([]);
      setPromoteChannel(undefined);
      setGuidedFile(null);
      setGuidedSig(null);
      queryClient.invalidateQueries({ queryKey: artifactsQueryOptions(slug, version).queryKey });
      queryClient.invalidateQueries({ queryKey: releasesQueryOptions(slug).queryKey });
      if (promoteChannel) {
        queryClient.invalidateQueries({
          queryKey: channelReleaseQueryOptions(slug, promoteChannel).queryKey,
        });
      }
    } catch (error) {
      // 标记尚未完成的文件为错误,保留已算 hash 便于重试(再次点上传即重跑)。
      setItems((prev) =>
        prev.map((it) =>
          it.status === "hashing" || it.status === "uploading"
            ? { ...it, status: "error", error: t`上传中断` }
            : it,
        ),
      );
      if (isApiError(error) && error.type === ERR_UPLOAD_CHECKSUM_MISMATCH) {
        notification.error({ message: t`校验和不符,上传被拒绝` });
      } else if (isApiError(error)) {
        notification.error({ message: t`上传失败`, description: error.detail });
      } else {
        notification.error({
          message: t`上传失败`,
          description: error instanceof Error ? error.message : String(error),
        });
      }
    } finally {
      setBusy(false);
    }
  }

  async function handleBatchUpload() {
    await uploadItems(items.filter((it) => it.status !== "done").map((it) => ({ ...it })));
  }

  // 引导式提交：用表单值 + 受控文件构造单个 StagedItem，走同一条上传链路。
  async function handleGuidedSubmit(values: GuidedValues): Promise<boolean> {
    if (!guidedFile) {
      notification.error({ message: t`请先选择要上传的产物文件` });
      return false;
    }
    const guidedKind =
      values.platform === "react-native-android"
        ? "universal"
        : classifyArtifact(guidedFile.name).kind;
    const staged: StagedItem = {
      uid: `${guidedFile.name}:${guidedFile.size}:${guidedFile.lastModified}`,
      file: guidedFile,
      platform: values.platform,
      kind: guidedKind,
      target: values.platform === "tauri-desktop" ? values.target : undefined,
      abi: values.platform === "react-native-android" ? values.abi : undefined,
      uncertain: false,
      signatureName: guidedSig?.name,
      signatureText: guidedSig ? (await guidedSig.text()).trim() : undefined,
      status: "pending",
      hashRatio: 0,
      uploadRatio: 0,
    };
    setItems([staged]);
    await uploadItems([staged]);
    return false;
  }

  const columns: TableColumnsType<StagedItem> = [
    {
      title: t`文件`,
      dataIndex: "uid",
      render: (_, it) => (
        <Space direction="vertical" size={0}>
          <span style={{ wordBreak: "break-all" }}>{it.file.name}</span>
          <span style={{ color: "rgba(0,0,0,0.45)", fontSize: 12 }}>
            {formatBytes(it.file.size)}
            {it.signatureName ? (
              <Tag color="green" style={{ marginLeft: 8 }}>
                <Trans>含签名</Trans>
              </Tag>
            ) : null}
          </span>
        </Space>
      ),
    },
    {
      title: t`平台`,
      width: 180,
      render: (_, it) => (
        <Space direction="vertical" size={4} style={{ width: "100%" }}>
          <Select<Platform>
            size="small"
            style={{ width: "100%" }}
            value={it.platform}
            disabled={busy}
            onChange={(v) => patch(it.uid, { platform: v, uncertain: false })}
            options={PLATFORMS.map((p) => ({ label: platformLabel(p), value: p }))}
          />
          {it.uncertain && (
            <Tag color="warning">
              <Trans>请确认类型</Trans>
            </Tag>
          )}
        </Space>
      ),
    },
    {
      title: "target / abi",
      width: 160,
      render: (_, it) =>
        it.platform === "react-native-android" ? (
          <Input
            size="small"
            placeholder="abi"
            value={it.abi}
            disabled={busy}
            onChange={(e) => patch(it.uid, { abi: e.target.value })}
          />
        ) : (
          <Input
            size="small"
            placeholder="target"
            value={it.target}
            disabled={busy}
            onChange={(e) => patch(it.uid, { target: e.target.value })}
          />
        ),
    },
    {
      title: t`进度`,
      width: 130,
      render: (_, it) => <StagedProgress item={it} />,
    },
    {
      title: "",
      width: 48,
      render: (_, it) => (
        <Button
          type="link"
          size="small"
          danger
          disabled={busy}
          onClick={() => setItems((prev) => prev.filter((p) => p.uid !== it.uid))}
        >
          <Trans>移除</Trans>
        </Button>
      ),
    },
  ];

  return (
    <div style={{ marginTop: 24 }}>
      <Card size="small" title={t`上传产物`}>
        <Space direction="vertical" size="middle" style={{ width: "100%" }}>
          <Segmented
            value={mode}
            onChange={(v) => setMode(v as "guided" | "batch")}
            options={[
              { label: t`引导式`, value: "guided" },
              { label: t`批量拖拽`, value: "batch" },
            ]}
          />

          {mode === "guided" ? (
            <ProForm<GuidedValues>
              layout="vertical"
              disabled={busy}
              submitter={{
                searchConfig: { submitText: t`上传并发布` },
                resetButtonProps: false,
                submitButtonProps: { loading: busy, icon: <UploadOutlined /> },
              }}
              onFinish={handleGuidedSubmit}
            >
              <ProFormSelect
                name="platform"
                label={t`平台`}
                options={PLATFORMS.map((p) => ({ label: platformLabel(p), value: p }))}
                rules={[{ required: true, message: t`请选择平台` }]}
              />
              <ProFormDependency name={["platform"]}>
                {({ platform }) =>
                  platform === "tauri-desktop" ? (
                    <>
                      <ProFormSelect
                        name="target"
                        label={t`目标架构`}
                        options={TAURI_TARGETS.map((tg) => ({
                          label: friendlyArch("tauri-desktop", tg, undefined),
                          value: tg,
                        }))}
                        rules={[{ required: true, message: t`请选择目标架构` }]}
                      />
                      <ProForm.Item label={t`安装包`} required>
                        <Upload
                          maxCount={1}
                          beforeUpload={(f) => {
                            setGuidedFile(f);
                            return false;
                          }}
                          onRemove={() => setGuidedFile(null)}
                          fileList={
                            guidedFile
                              ? [{ uid: "pkg", name: guidedFile.name, status: "done" }]
                              : []
                          }
                        >
                          <Button icon={<UploadOutlined />}>
                            <Trans>选择安装包</Trans>
                          </Button>
                        </Upload>
                      </ProForm.Item>
                      <ProForm.Item label={t`签名 .sig（可选）`}>
                        <Upload
                          maxCount={1}
                          beforeUpload={(f) => {
                            setGuidedSig(f);
                            return false;
                          }}
                          onRemove={() => setGuidedSig(null)}
                          fileList={
                            guidedSig ? [{ uid: "sig", name: guidedSig.name, status: "done" }] : []
                          }
                        >
                          <Button icon={<UploadOutlined />}>
                            <Trans>选择 .sig</Trans>
                          </Button>
                        </Upload>
                      </ProForm.Item>
                    </>
                  ) : platform === "react-native-android" ? (
                    <>
                      <ProFormSelect
                        name="abi"
                        label="ABI"
                        options={ANDROID_ABIS.map((a) => ({ label: a, value: a }))}
                        rules={[{ required: true, message: t`请选择 ABI` }]}
                      />
                      <ProForm.Item label="APK" required>
                        <Upload
                          maxCount={1}
                          beforeUpload={(f) => {
                            setGuidedFile(f);
                            return false;
                          }}
                          onRemove={() => setGuidedFile(null)}
                          fileList={
                            guidedFile
                              ? [{ uid: "apk", name: guidedFile.name, status: "done" }]
                              : []
                          }
                        >
                          <Button icon={<UploadOutlined />}>
                            <Trans>选择 APK</Trans>
                          </Button>
                        </Upload>
                      </ProForm.Item>
                      <Alert
                        type="info"
                        showIcon
                        message={<Trans>versionCode 在版本信息里设置（创建 / 编辑版本时）。</Trans>}
                      />
                    </>
                  ) : null
                }
              </ProFormDependency>
            </ProForm>
          ) : (
            <Upload.Dragger
              multiple
              showUploadList={false}
              disabled={busy}
              beforeUpload={(file, batch) => {
                // beforeUpload 每文件触发一次但 batch 是整批;在最后一个文件时整批处理。
                if (file === batch[batch.length - 1]) void ingest(Array.from(batch));
                return Upload.LIST_IGNORE;
              }}
            >
              <p className="ant-upload-drag-icon">
                <InboxOutlined />
              </p>
              <p className="ant-upload-text">
                <Trans>拖拽或点击选择产物文件</Trans>
              </p>
              <p className="ant-upload-hint">
                <Trans>
                  支持一次拖入多个平台 / 架构的产物;Tauri 的 .sig 与同名 bundle 一起选会自动配对
                </Trans>
              </p>
            </Upload.Dragger>
          )}

          {items.length > 0 && (
            <>
              <Table<StagedItem>
                rowKey="uid"
                size="small"
                pagination={false}
                dataSource={items}
                columns={columns}
              />

              {mode === "batch" && (
                <Space wrap>
                  <Checkbox
                    checked={publish}
                    disabled={busy}
                    onChange={(e) => setPublish(e.target.checked)}
                  >
                    <Trans>上传后发布</Trans>
                  </Checkbox>
                  {publish && canPromote && (
                    <Select
                      allowClear
                      style={{ width: 220 }}
                      placeholder={t`发布后 promote 到 channel(可选)`}
                      value={promoteChannel}
                      disabled={busy}
                      onChange={(v) => setPromoteChannel(v)}
                      options={(channelsQuery.data ?? []).map((c) => ({
                        label: c.name,
                        value: c.name,
                      }))}
                    />
                  )}
                  <Button
                    type="primary"
                    icon={<UploadOutlined />}
                    loading={busy}
                    onClick={handleBatchUpload}
                  >
                    <Trans>上传</Trans>
                  </Button>
                </Space>
              )}

              <Alert
                type="info"
                showIcon
                message={
                  <Trans>
                    直传需要对象存储桶已配置 CORS。若上传报网络错误,请到设置 · 存储为当前后端配置
                    CORS。
                  </Trans>
                }
              />
            </>
          )}
        </Space>
      </Card>
    </div>
  );
}

// 单个暂存文件的 hash / 上传进度展示。
function StagedProgress({ item }: { item: StagedItem }) {
  if (item.status === "error") {
    return (
      <Tag color="red" title={item.error}>
        <Trans>失败</Trans>
      </Tag>
    );
  }
  if (item.status === "done") {
    return (
      <Tag color="green">
        <Trans>完成</Trans>
      </Tag>
    );
  }
  if (item.status === "hashing" || item.status === "uploading") {
    const ratio = item.status === "hashing" ? item.hashRatio : item.uploadRatio;
    return <Progress percent={Math.round(ratio * 100)} size="small" status="active" />;
  }
  return (
    <Tag>
      <Trans>待上传</Trans>
    </Tag>
  );
}
