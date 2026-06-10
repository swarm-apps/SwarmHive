import { $api, type components, fetchClient } from ".";

// Registration policy singleton (add-registration-policy-and-self-register).

export type RegistrationPolicy = components["schemas"]["RegistrationPolicy"];
export type UpdateRegistrationPolicyReq = components["schemas"]["UpdateRegistrationPolicyReq"];

export const REGISTRATION_POLICY_PATH = "/api/v1/auth/registration-policy" as const;

/** Admin 读取注册策略(requires auth:manage)。 */
export function registrationPolicyQueryOptions() {
  return $api.queryOptions("get", REGISTRATION_POLICY_PATH);
}

export async function updateRegistrationPolicy(
  body: UpdateRegistrationPolicyReq,
): Promise<RegistrationPolicy> {
  const { data, error } = await fetchClient.PUT(REGISTRATION_POLICY_PATH, { body });
  if (error) throw error;
  if (!data) throw new Error("update registration policy: missing body");
  return data;
}
