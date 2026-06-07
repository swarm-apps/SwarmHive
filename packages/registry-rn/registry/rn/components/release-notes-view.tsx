// release-notes-view —— release notes 渲染槽(NativeWind + RNR Text)。缺省用 <Text> 纯文本
// 渲染(ScrollView 保留换行,等价 web 的 whitespace-pre-wrap);通过 `renderer` 接入 Markdown
// 渲染器(如 react-native-markdown-display)。镜像 registry-web 的 props 形态与 token 文本色
// (text-muted-foreground text-sm);背景 / 圆角由各父组件的 bg-muted 盒子负责,本组件不带底色。
// registry:component。registryDependency: @react-native-reusables/text。

import type { ReactNode } from "react";
import { ScrollView } from "react-native";
import { Text } from "@/components/ui/text";

export interface ReleaseNotesViewProps {
  notes?: string;
  /** 自定义渲染(如接 Markdown 渲染器);缺省按纯文本渲染(保留换行)。 */
  renderer?: (notes: string) => ReactNode;
  /** 覆盖容器最大高度,默认 220。 */
  maxHeight?: number;
}

export function ReleaseNotesView({ notes, renderer, maxHeight = 220 }: ReleaseNotesViewProps) {
  if (!notes) return null;
  return (
    <ScrollView style={{ maxHeight }} contentContainerClassName="pr-1" showsVerticalScrollIndicator>
      {renderer ? (
        renderer(notes)
      ) : (
        <Text className="text-muted-foreground text-sm leading-5">{notes}</Text>
      )}
    </ScrollView>
  );
}
