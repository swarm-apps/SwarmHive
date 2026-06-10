import { ProFormSelect } from "@ant-design/pro-components";
import { useLingui } from "@lingui/react/macro";
import { fetchClient } from "@/lib/api";
import type { Role } from "@/lib/api/account";

/** 角色选择(排除 owner;server 端同样拒绝)。邀请抽屉、「更改角色」与审批 Modal 共用。 */
export function RoleSelect({ label }: { label?: string }) {
  const { t } = useLingui();
  return (
    <ProFormSelect
      name="role_id"
      label={label ?? t`授予角色`}
      rules={[{ required: true, message: t`请选择角色` }]}
      request={async () => {
        const { data, error } = await fetchClient.GET("/api/v1/roles");
        if (error) throw error;
        return (data ?? [])
          .filter((r: Role) => r.name !== "owner")
          .map((r: Role) => ({ label: r.name, value: r.id }));
      }}
    />
  );
}
