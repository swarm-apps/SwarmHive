// vendored-for-typecheck —— 镜像 React Native Reusables / shadcn 的 cn() 工具。
// 仅供 registry-rn 本地 tsc 解析 `@/lib/utils` 用,【不列入 registry.json items】;
// 消费者经 RNR init / 任一 RNR 组件已自带 lib/utils(同款 clsx + tailwind-merge)。
import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
