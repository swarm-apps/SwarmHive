import { useLingui } from "@lingui/react/macro";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { App } from "antd";
import { isApiError } from "@/lib/api";
import {
  ERR_EMAIL_ALREADY_VERIFIED,
  ERR_MAIL_NOT_CONFIGURED,
  ERR_RATE_LIMITED,
  postResendVerifyEmail,
} from "@/lib/api/account";
import { meQueryOptions } from "./meQuery";

/**
 * Shared by the verify banner (`_auth/route.tsx`) and the Settings → Account
 * tab. Wraps `POST /users/me/verify-email/send` with the three typed-error
 * branches the endpoint can emit, plus a /me invalidate so an
 * already-verified race makes the banner disappear.
 */
export function useResendVerify() {
  const { t } = useLingui();
  const { notification } = App.useApp();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => postResendVerifyEmail(),
    onSuccess: () => {
      notification.success({
        message: t`验证邮件已发送`,
        description: t`请检查收件箱，链接 24 小时内有效。`,
      });
    },
    onError: async (error) => {
      if (isApiError(error)) {
        switch (error.type) {
          case ERR_EMAIL_ALREADY_VERIFIED:
            notification.info({ message: t`邮箱已验证，无需重发。` });
            // Banner reads /me — refresh so it disappears.
            await queryClient.invalidateQueries({ queryKey: meQueryOptions().queryKey });
            return;
          case ERR_RATE_LIMITED:
            notification.warning({
              message: t`发送过于频繁`,
              description: t`请稍等一分钟后再试。`,
            });
            return;
          case ERR_MAIL_NOT_CONFIGURED:
            notification.error({
              message: t`邮件未配置`,
              description: t`请先在“设置 → 邮件”中配置 SMTP 服务商。`,
            });
            return;
        }
      }
      notification.error({ message: t`发送失败，请稍后重试。` });
    },
  });
}
