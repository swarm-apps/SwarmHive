import { PlusOutlined } from "@ant-design/icons";
import {
  DrawerForm,
  PageContainer,
  type ProColumns,
  ProFormDigit,
  type ProFormInstance,
  ProFormSelect,
  ProFormSwitch,
  ProFormText,
  ProTable,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { App, Button, Space, Tag } from "antd";
import { useRef, useState } from "react";
import { isApiError } from "@/lib/api";
import {
  activateBackend,
  backendsQueryOptions,
  type CreateStorageBackendRequest,
  createBackend,
  STORAGE_PRESETS,
  type StorageBackendView,
  type StoragePresetKey,
  testBackend,
  type UpdateStorageBackendRequest,
  updateBackend,
} from "@/lib/api/storage";
import { usePermissions } from "@/lib/query/usePermissions";

export const Route = createFileRoute("/_auth/settings/storage")({
  component: StoragePage,
});

// 表单值 = 创建请求 + 一个不提交的预设字段。
type FormValues = CreateStorageBackendRequest & { __preset?: StoragePresetKey };

function StoragePage() {
  const { t } = useLingui();
  const { notification, modal } = App.useApp();
  const queryClient = useQueryClient();
  const { has } = usePermissions();

  const backendsQuery = useQuery(backendsQueryOptions());
  const backends = backendsQuery.data ?? [];

  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editing, setEditing] = useState<StorageBackendView | null>(null);

  const canManage = has("storage:manage");

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: backendsQueryOptions().queryKey });

  function openCreate() {
    setEditing(null);
    setDrawerOpen(true);
  }

  function openEdit(row: StorageBackendView) {
    setEditing(row);
    setDrawerOpen(true);
  }

  async function handleSubmit(values: FormValues): Promise<boolean> {
    try {
      if (editing) {
        const body: UpdateStorageBackendRequest = {
          name: values.name,
          endpoint: values.endpoint,
          bucket: values.bucket,
          region: values.region,
          access_key_id: values.access_key_id,
          force_path_style: values.force_path_style,
          prefix: values.prefix,
          public_base_url: values.public_base_url,
          url_mode: values.url_mode,
          signed_url_ttl_secs: values.signed_url_ttl_secs,
        };
        // 留空 = 不改：不带该字段，避免 server 用空串覆盖已存 secret。
        if (values.access_key_secret) {
          body.access_key_secret = values.access_key_secret;
        }
        await updateBackend(editing.id, body);
        notification.success({ message: t`存储后端已更新` });
      } else {
        await createBackend({
          name: values.name,
          endpoint: values.endpoint,
          bucket: values.bucket,
          region: values.region,
          access_key_id: values.access_key_id,
          access_key_secret: values.access_key_secret,
          force_path_style: values.force_path_style,
          prefix: values.prefix,
          public_base_url: values.public_base_url,
          url_mode: values.url_mode,
          signed_url_ttl_secs: values.signed_url_ttl_secs,
        });
        notification.success({ message: t`存储后端已创建` });
      }
      setDrawerOpen(false);
      setEditing(null);
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

  async function handleTest(row: StorageBackendView): Promise<void> {
    try {
      const result = await testBackend(row.id);
      if (result.ok) {
        notification.success({
          message: t`连通正常`,
          description: result.supports_sha256_checksum
            ? t`该后端支持 sha256 校验`
            : t`该后端不支持 sha256 校验（将回退 Content-MD5）`,
        });
      } else {
        notification.error({ message: t`连通失败`, description: result.detail });
      }
      await invalidate();
    } catch (error) {
      notification.error({
        message: t`测试失败`,
        description: isApiError(error) ? error.detail : String(error),
      });
    }
  }

  function handleActivate(row: StorageBackendView) {
    modal.confirm({
      title: t`激活该存储后端？`,
      content: t`激活后会立即作为当前存储通道，并把其他后端设为未激活。`,
      okText: t`激活`,
      cancelText: t`取消`,
      onOk: async () => {
        try {
          await activateBackend(row.id);
          notification.success({ message: t`已激活`, description: row.name });
          await invalidate();
        } catch (error) {
          notification.error({
            message: t`激活失败`,
            description: isApiError(error) ? error.detail : String(error),
          });
        }
      },
    });
  }

  const columns: ProColumns<StorageBackendView>[] = [
    { title: t`名称`, dataIndex: "name" },
    { title: "Endpoint", dataIndex: "endpoint", ellipsis: true },
    { title: "Bucket", dataIndex: "bucket" },
    {
      title: t`状态`,
      dataIndex: "active",
      width: 100,
      render: (_, row) =>
        row.active ? (
          <Tag color="green">
            <Trans>激活中</Trans>
          </Tag>
        ) : (
          <Tag>
            <Trans>未激活</Trans>
          </Tag>
        ),
    },
    {
      title: t`URL 模式`,
      dataIndex: "url_mode",
      width: 110,
      render: (_, row) => (row.url_mode === "public" ? t`公开直链` : t`预签名`),
    },
    {
      title: "sha256",
      dataIndex: "supports_sha256_checksum",
      width: 90,
      render: (_, row) =>
        row.connectivity_status == null ? (
          <span style={{ color: "rgba(0,0,0,0.45)" }}>
            <Trans>未自检</Trans>
          </span>
        ) : row.supports_sha256_checksum ? (
          <Tag color="blue">
            <Trans>支持</Trans>
          </Tag>
        ) : (
          <Tag>
            <Trans>不支持</Trans>
          </Tag>
        ),
    },
    {
      title: t`操作`,
      width: 200,
      render: (_, row) =>
        canManage ? (
          <Space size="small">
            <Button type="link" size="small" onClick={() => handleTest(row)}>
              <Trans>测试</Trans>
            </Button>
            <Button type="link" size="small" onClick={() => openEdit(row)}>
              <Trans>编辑</Trans>
            </Button>
            {!row.active && (
              <Button type="link" size="small" onClick={() => handleActivate(row)}>
                <Trans>激活</Trans>
              </Button>
            )}
          </Space>
        ) : null,
    },
  ];

  return (
    <PageContainer title={t`存储`} breadcrumbRender={false}>
      <ProTable<StorageBackendView>
        rowKey="id"
        search={false}
        options={false}
        loading={backendsQuery.isPending}
        dataSource={backends}
        pagination={{ pageSize: 20, hideOnSinglePage: true }}
        locale={{
          emptyText: <Trans>还没有配置存储后端，点击右上角新建并激活后即可发布</Trans>,
        }}
        toolBarRender={() =>
          canManage
            ? [
                <Button key="create" type="primary" icon={<PlusOutlined />} onClick={openCreate}>
                  <Trans>新建存储后端</Trans>
                </Button>,
              ]
            : []
        }
        columns={columns}
      />

      <BackendDrawer
        editing={editing}
        open={drawerOpen}
        onOpenChange={(o) => {
          setDrawerOpen(o);
          if (!o) setEditing(null);
        }}
        onFinish={handleSubmit}
      />
    </PageContainer>
  );
}

function BackendDrawer({
  editing,
  open,
  onOpenChange,
  onFinish,
}: {
  editing: StorageBackendView | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onFinish: (v: FormValues) => Promise<boolean>;
}) {
  const { t } = useLingui();
  const formRef = useRef<ProFormInstance>(undefined);
  const [endpointHint, setEndpointHint] = useState("");

  return (
    <DrawerForm<FormValues>
      formRef={formRef}
      // key remount：切换「新建 / 编辑不同行」时让 initialValues 重新生效。
      key={editing?.id ?? "new"}
      title={editing ? t`编辑存储后端` : t`新建存储后端`}
      open={open}
      onOpenChange={onOpenChange}
      drawerProps={{ destroyOnClose: true }}
      initialValues={
        editing
          ? {
              name: editing.name,
              endpoint: editing.endpoint,
              bucket: editing.bucket,
              region: editing.region,
              access_key_id: editing.access_key_id,
              force_path_style: editing.force_path_style,
              prefix: editing.prefix ?? undefined,
              public_base_url: editing.public_base_url ?? undefined,
              url_mode: editing.url_mode,
              signed_url_ttl_secs: editing.signed_url_ttl_secs,
            }
          : { url_mode: "signed", signed_url_ttl_secs: 600, force_path_style: false }
      }
      onFinish={onFinish}
    >
      {!editing && (
        <ProFormSelect
          name="__preset"
          label={t`预设`}
          tooltip={t`仅预填 path-style 与 URL 模式，其余仍需手填`}
          options={(Object.keys(STORAGE_PRESETS) as StoragePresetKey[]).map((key) => ({
            label: STORAGE_PRESETS[key].label,
            value: key,
          }))}
          fieldProps={{
            onChange: (value: StoragePresetKey) => {
              const preset = STORAGE_PRESETS[value];
              if (!preset) return;
              formRef.current?.setFieldsValue({
                force_path_style: preset.force_path_style,
                url_mode: preset.url_mode,
              });
              setEndpointHint(preset.endpointHint);
            },
          }}
        />
      )}
      <ProFormText
        name="name"
        label={t`名称`}
        rules={[{ required: true, message: t`请输入名称` }]}
      />
      <ProFormText
        name="endpoint"
        label="Endpoint"
        placeholder={endpointHint || undefined}
        rules={[{ required: true, message: t`请输入 endpoint` }]}
      />
      <ProFormText
        name="bucket"
        label="Bucket"
        rules={[{ required: true, message: t`请输入 bucket` }]}
      />
      <ProFormText
        name="region"
        label="Region"
        rules={[{ required: true, message: t`请输入 region` }]}
      />
      <ProFormText
        name="access_key_id"
        label="Access Key ID"
        rules={[{ required: true, message: t`请输入 Access Key ID` }]}
      />
      <ProFormText.Password
        name="access_key_secret"
        label="Access Key Secret"
        placeholder={editing ? t`留空表示不修改` : undefined}
        rules={editing ? [] : [{ required: true, message: t`请输入 Access Key Secret` }]}
      />
      <ProFormSwitch name="force_path_style" label={t`Path-style 寻址`} />
      <ProFormSelect
        name="url_mode"
        label={t`URL 模式`}
        options={[
          { label: t`公开直链 (public)`, value: "public" },
          { label: t`预签名 (signed)`, value: "signed" },
        ]}
        rules={[{ required: true, message: t`请选择 URL 模式` }]}
      />
      <ProFormDigit
        name="signed_url_ttl_secs"
        label={t`预签名有效期（秒）`}
        min={1}
        fieldProps={{ precision: 0 }}
      />
      <ProFormText name="prefix" label={t`对象前缀（可选）`} />
      <ProFormText name="public_base_url" label={t`公开访问根 URL（可选）`} />
    </DrawerForm>
  );
}
