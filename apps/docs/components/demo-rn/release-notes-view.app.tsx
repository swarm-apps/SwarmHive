// 扁平化单文件 Snack demo —— release-notes-view。
// 由 add-docs-rn-snack 产出:组件本体（ReleaseNotesView）逐字内联自
// packages/registry-rn/registry/rn/components/release-notes-view.tsx;App 是 demo
// scaffolding（同 web docs 范式）。此组件纯展示（ScrollView + Text 渲染 notes），
// 不依赖 useUpdate/engine/mock,故无需内联 update-texts/context/provider。
// 不依赖 @swarm-hive/sdk（平台无关、零外部依赖);此文件不被 docs tsconfig 编译,
// 只作 Snack 源码（codegen 读取）。
import type { ReactNode } from "react";
import { ScrollView, StyleSheet, Text, View } from "react-native";

// ============ ReleaseNotesView（逐字内联自 registry-rn components/release-notes-view.tsx）============
function ReleaseNotesView({
  notes,
  renderer,
  maxHeight = 220,
}: {
  notes?: string;
  /** 自定义渲染(如接 Markdown 渲染器);缺省按纯文本渲染(保留换行)。 */
  renderer?: (notes: string) => ReactNode;
  /** 覆盖容器最大高度,默认 220。 */
  maxHeight?: number;
}) {
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

// ============ App（demo:直接渲染 ReleaseNotesView 展示一段多行 markdown）============
const SAMPLE = [
  "## SwarmHive 1.4.0",
  "",
  "- ✨ 增量下载，包体减少约 60%",
  "- 🐛 修复离线检查死循环",
  "- ⚡ 启动优化，冷启动快 30%",
  "",
  "### 升级说明",
  "",
  "本次更新无需手动迁移，重启应用即可生效。",
  "如遇异常请回滚到 1.3.x 并反馈日志。",
].join("\n");

export default function App() {
  return (
    <View style={appStyles.root}>
      <Text style={appStyles.heading}>ReleaseNotesView</Text>
      <View style={appStyles.demo}>
        <ReleaseNotesView notes={SAMPLE} />
      </View>
    </View>
  );
}
const appStyles = StyleSheet.create({
  root: {
    backgroundColor: "#E2E8F0",
    flex: 1,
    gap: 12,
    justifyContent: "center",
    padding: 16,
  },
  heading: { color: "#0F172A", fontSize: 16, fontWeight: "700" },
  demo: { backgroundColor: "#FFFFFF", borderRadius: 16, padding: 16 },
});
