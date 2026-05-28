import type { useLingui } from "@lingui/react/macro";
import type { FormRule } from "antd";

type T = ReturnType<typeof useLingui>["t"];

/**
 * Shared with setup / reset-password / accept-invite. Mirrors the server-side
 * `password-too-weak` policy: ≥12 chars, ≥3 character classes, not in the
 * top-100 weak-password dictionary. Server is the source of truth — these
 * client rules only catch the obvious cases without round-trip.
 */
export function passwordRules(t: T): FormRule[] {
  return [
    { required: true, message: t`请输入密码` },
    { min: 12, message: t`密码至少 12 个字符` },
  ];
}

/**
 * Antd validator factory for the confirm-password field. Pair with
 * `dependencies: ["password"]` on the Form.Item.
 */
export function confirmPasswordRules(t: T, getPassword: () => string | undefined): FormRule[] {
  return [
    { required: true, message: t`请再次输入密码` },
    () => ({
      validator: (_, value) =>
        !value || value === getPassword()
          ? Promise.resolve()
          : Promise.reject(new Error(t`两次输入的密码不一致`)),
    }),
  ];
}
