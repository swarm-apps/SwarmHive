// release-notes-view —— release notes 渲染槽(纯 RN 原语)。缺省用 <Text> 纯文本渲染
// (保留换行 whitespace-pre-wrap 等价为 ScrollView + Text);通过 `renderer` 接入
// Markdown 渲染器(如 react-native-markdown-display)。镜像 tauri 版的 props 形态,
// 但用 View/Text/ScrollView 替 div。registry:component。

import type { ReactNode } from "react";
import { ScrollView, StyleSheet, Text } from "react-native";

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
    <ScrollView
      style={[styles.scroll, { maxHeight }]}
      contentContainerStyle={styles.content}
      showsVerticalScrollIndicator
    >
      {renderer ? renderer(notes) : <Text style={styles.text}>{notes}</Text>}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  scroll: {
    backgroundColor: "#F8FAFC",
    borderRadius: 10,
  },
  content: {
    padding: 12,
  },
  text: {
    color: "#0F172A",
    fontSize: 13,
    lineHeight: 19,
  },
});
