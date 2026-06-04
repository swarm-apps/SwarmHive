// 仅供 registry-web 本地 typecheck 的 Button 占位 —— **不分发**(不在 registry.json items)。
// 消费者经 registryDependencies `["button"]` 从 @shadcn 装 canonical 版本。
// 这里只需 API(variant/size + button 属性)与 canonical 对齐,样式不重要。

import * as React from "react";
import { cn } from "@/lib/utils";

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "default" | "secondary" | "destructive" | "outline" | "ghost" | "link";
  size?: "default" | "sm" | "lg" | "icon";
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = "default", size = "default", type = "button", ...props }, ref) => (
    <button
      ref={ref}
      type={type}
      data-variant={variant}
      data-size={size}
      className={cn("inline-flex items-center justify-center", className)}
      {...props}
    />
  ),
);
Button.displayName = "Button";
