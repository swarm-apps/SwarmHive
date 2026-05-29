import { Trans } from "@lingui/react/macro";
import { Button, Result } from "antd";
import type { FallbackProps } from "react-error-boundary";

export function GlobalErrorFallback({ error, resetErrorBoundary }: FallbackProps) {
  const message = error instanceof Error ? error.message : String(error);
  return (
    <Result
      status="error"
      title={<Trans>页面出错了</Trans>}
      subTitle={message}
      extra={
        <Button type="primary" onClick={resetErrorBoundary}>
          <Trans>重试</Trans>
        </Button>
      }
    />
  );
}
