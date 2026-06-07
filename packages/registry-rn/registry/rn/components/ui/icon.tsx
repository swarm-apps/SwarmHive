// vendored-for-typecheck —— 镜像 React Native Reusables 的 icon（nativewind styled 包 Lucide）。
// 仅供 registry-rn 本地 tsc 解析用,【不列入 registry.json items】;
// 消费者经 @react-native-reusables/icon（被 dialog 传递依赖）拿 canonical 版本。
import type { LucideIcon, LucideProps } from "lucide-react-native";
import { styled } from "nativewind";
import * as React from "react";
import { TextClassContext } from "@/components/ui/text";
import { cn } from "@/lib/utils";

type IconProps = LucideProps & {
  as: LucideIcon;
};

function IconImpl({ as: IconComponent, ...props }: IconProps) {
  return <IconComponent {...props} />;
}

const StyledIconImpl = styled(IconImpl);

/**
 * A wrapper component for Lucide icons with Nativewind `className` support via `styled`.
 */
function Icon({ as: IconComponent, className, size = 14, ...props }: IconProps) {
  const textClass = React.useContext(TextClassContext);
  return (
    <StyledIconImpl
      as={IconComponent}
      className={cn("text-foreground", textClass, className)}
      size={size}
      {...props}
    />
  );
}

export { Icon };
