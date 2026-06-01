import { $api, type components, fetchClient } from ".";

// RFC 8628 device authorization — the `/device` approval page (add-cli-device-login).

export type DeviceAuthorizationView = components["schemas"]["DeviceAuthorizationView"];

export const DEVICE_LOOKUP_PATH = "/api/v1/auth/device/lookup" as const;

/** Look up a pending device grant by its user code (authenticated). */
export function deviceLookupQueryOptions(userCode: string) {
  return $api.queryOptions(
    "get",
    DEVICE_LOOKUP_PATH,
    { params: { query: { user_code: userCode } } },
    { staleTime: 5_000, retry: false, enabled: userCode.length > 0 },
  );
}

export async function postDeviceApprove(userCode: string): Promise<void> {
  const { error } = await fetchClient.POST("/api/v1/auth/device/approve", {
    body: { user_code: userCode },
  });
  if (error) throw error;
}

export async function postDeviceDeny(userCode: string): Promise<void> {
  const { error } = await fetchClient.POST("/api/v1/auth/device/deny", {
    body: { user_code: userCode },
  });
  if (error) throw error;
}

/**
 * Build the `next` path that survives the login round-trip with the user code
 * intact. Pure + exported for unit testing.
 */
export function deviceLoginNext(userCode: string | undefined): string {
  return userCode ? `/device?user_code=${encodeURIComponent(userCode)}` : "/device";
}
