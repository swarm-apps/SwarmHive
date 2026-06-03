import {
  DrawerForm,
  PageContainer,
  ProFormCheckbox,
  ProFormText,
} from "@ant-design/pro-components";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link, Outlet, redirect, useRouterState } from "@tanstack/react-router";
import { App, Button, Popconfirm, Space, Tag } from "antd";
import { useState } from "react";
import { isApiError } from "@/lib/api";
import {
  type App as AppModel,
  appsQueryOptions,
  deleteApp,
  ERR_APP_HAS_RELEASES,
  PLATFORMS,
  type Platform,
  platformLabel,
  updateApp,
} from "@/lib/api/apps";
import { usePermissions } from "@/lib/query/usePermissions";

export const Route = createFileRoute("/_auth/apps/$slug")({
  // 详情外壳：先确认 app 存在（复用 apps 列表 query，零后端新增），404 兜底回列表。
  beforeLoad: async ({ context, params }) => {
    const apps = await context.queryClient.ensureQueryData(appsQueryOptions());
    if (!apps.some((a) => a.slug === params.slug)) {
      throw redirect({ to: "/apps", replace: true });
    }
  },
  component: AppDetailShell,
});

interface AppFormValues {
  slug: string;
  display_name: string;
  platforms: Platform[];
}

function AppDetailShell() {
  const { t } = useLingui();
  const { slug } = Route.useParams();
  const navigate = Route.useNavigate();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();
  const { has } = usePermissions();

  const appsQuery = useQuery(appsQueryOptions());
  const app = appsQuery.data?.find((a) => a.slug === slug);

  const [editOpen, setEditOpen] = useState(false);

  // 订阅 location 决定当前 tab —— 不能用 router 快照（admin-spa.md：useRouterState）。
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const activeTab = pathname.endsWith("/channels") ? "channels" : "releases";
  // release 详情页 /apps/:slug/releases/:version —— 面包屑末段延伸到版本号（列表不匹配）。
  const releaseDetail = pathname.match(/\/releases\/([^/]+)$/);
  const detailVersion = releaseDetail ? decodeURIComponent(releaseDetail[1]) : null;

  const canUpdate = has("app:update");
  const canDelete = has("app:delete");

  const invalidateApps = () =>
    queryClient.invalidateQueries({ queryKey: appsQueryOptions().queryKey });

  async function handleEdit(values: AppFormValues): Promise<boolean> {
    if (!app) return false;
    try {
      await updateApp(app.slug, {
        display_name: values.display_name.trim(),
        platforms: values.platforms,
      });
      notification.success({ message: t`应用已更新` });
      await invalidateApps();
      setEditOpen(false);
      return true;
    } catch {
      notification.error({ message: t`更新失败，请稍后重试` });
      return false;
    }
  }

  async function handleDelete(): Promise<void> {
    if (!app) return;
    try {
      await deleteApp(app.slug);
      notification.success({ message: t`应用已删除` });
      await invalidateApps();
      navigate({ to: "/apps" });
    } catch (error) {
      if (isApiError(error) && error.type === ERR_APP_HAS_RELEASES) {
        notification.error({ message: t`该应用下仍有版本，无法删除` });
        return;
      }
      notification.error({ message: t`删除失败，请稍后重试` });
    }
  }

  // beforeLoad 已保证 app 存在；加载态短暂返回 null。
  if (!app) return null;

  return (
    <PageContainer
      title={app.display_name}
      subTitle={
        <Space size={4} wrap>
          <span style={{ color: "rgba(0,0,0,0.45)" }}>{app.slug}</span>
          {app.platforms.map((p) => (
            <Tag key={p} color="blue">
              {platformLabel(p)}
            </Tag>
          ))}
        </Space>
      }
      breadcrumb={{
        items: [
          { title: <Link to="/apps">{t`应用`}</Link> },
          { title: app.display_name },
          ...(detailVersion
            ? [
                {
                  title: (
                    <Link to="/apps/$slug/releases" params={{ slug }}>
                      {t`版本`}
                    </Link>
                  ),
                },
                { title: detailVersion },
              ]
            : [{ title: activeTab === "channels" ? t`渠道` : t`版本` }]),
        ],
      }}
      tabList={[
        { tab: t`版本`, key: "releases" },
        { tab: t`渠道`, key: "channels" },
      ]}
      tabActiveKey={activeTab}
      onTabChange={(key) => {
        if (key === "channels") {
          navigate({ to: "/apps/$slug/channels", params: { slug } });
        } else {
          navigate({ to: "/apps/$slug/releases", params: { slug } });
        }
      }}
      extra={[
        canUpdate ? (
          <Button key="edit" onClick={() => setEditOpen(true)}>
            <Trans>编辑</Trans>
          </Button>
        ) : null,
        canDelete ? (
          <Popconfirm
            key="delete"
            title={t`确认删除该应用？`}
            onConfirm={handleDelete}
            okText={t`删除`}
            cancelText={t`取消`}
          >
            <Button danger>
              <Trans>删除</Trans>
            </Button>
          </Popconfirm>
        ) : null,
      ].filter(Boolean)}
    >
      <Outlet />

      <EditAppDrawer app={editOpen ? app : null} onOpenChange={setEditOpen} onFinish={handleEdit} />
    </PageContainer>
  );
}

function EditAppDrawer({
  app,
  onOpenChange,
  onFinish,
}: {
  app: AppModel | null;
  onOpenChange: (open: boolean) => void;
  onFinish: (v: AppFormValues) => Promise<boolean>;
}) {
  const { t } = useLingui();
  return (
    <DrawerForm<AppFormValues>
      // key remount：切换不同行/重开时让 initialValues 每次重新生效（admin-spa.md 坑）。
      key={app?.slug ?? "none"}
      title={t`编辑应用`}
      open={app != null}
      onOpenChange={onOpenChange}
      drawerProps={{ destroyOnClose: true }}
      initialValues={app ? { display_name: app.display_name, platforms: app.platforms } : undefined}
      onFinish={onFinish}
    >
      <ProFormText label={t`Slug`} fieldProps={{ value: app?.slug, disabled: true }} />
      <ProFormText
        name="display_name"
        label={t`名称`}
        rules={[{ required: true, message: t`请输入名称` }]}
      />
      <ProFormCheckbox.Group
        name="platforms"
        label={t`平台`}
        options={PLATFORMS.map((p) => ({ label: platformLabel(p), value: p }))}
        rules={[{ required: true, message: t`请至少选择一个平台` }]}
      />
    </DrawerForm>
  );
}
