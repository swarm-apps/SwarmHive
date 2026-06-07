// vendored-for-typecheck —— 镜像 React Native Reusables 的 native-only-animated-view。
// 仅供 registry-rn 本地 tsc 解析用,【不列入 registry.json items】;
// 消费者经 @react-native-reusables/native-only-animated-view（被 dialog/alert-dialog 传递依赖）拿。
import { Platform } from "react-native";
import Animated from "react-native-reanimated";

/**
 * This component is used to wrap animated views that should only be animated on native.
 */
function NativeOnlyAnimatedView(props: React.ComponentProps<typeof Animated.View>) {
  if (Platform.OS === "web") {
    return <>{props.children as React.ReactNode}</>;
  } else {
    return <Animated.View {...props} />;
  }
}

export { NativeOnlyAnimatedView };
