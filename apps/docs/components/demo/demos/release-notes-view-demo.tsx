"use client";

// ReleaseNotesView 的 live demo:纯展示组件,不需要 engine,直接喂 notes。

import { ReleaseNotesView } from "@/components/release-notes-view";

const NOTES = [
  "## SwarmHive 1.4.0",
  "",
  "- ✨ 新增增量下载，更新包体积减少约 60%",
  "- 🐛 修复离线状态下检查更新的死循环",
  "- ⚡ 启动速度优化",
  "",
  "完整变更见 GitHub Releases。",
].join("\n");

export default function ReleaseNotesViewDemo() {
  return (
    <div className="w-full max-w-sm rounded-lg border bg-muted p-4">
      <ReleaseNotesView notes={NOTES} />
    </div>
  );
}
