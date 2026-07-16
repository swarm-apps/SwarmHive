// registry build 产物校验 —— 断言 public/r 的 JSON 已 inline content + registryDependencies 链正确。
// 这些文件随仓库提交(GitHub raw 分发的就是它们),故测试直接读取。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const rDir = fileURLToPath(new URL("../public/r/", import.meta.url));

function load(name: string): {
  files?: { content?: string }[];
  items?: unknown[];
  dependencies?: string[];
  registryDependencies?: string[];
} {
  return JSON.parse(readFileSync(`${rDir}${name}`, "utf8"));
}

describe("registry build output", () => {
  it("registry.json 索引 11 个 item", () => {
    expect(load("registry.json").items).toHaveLength(11);
  });

  // 两个弹窗的互斥判据都来自 update-dialog-visibility。漏声明这条依赖 → 下游拉取时不会带上
  // 该 lib,组件 import 一个不存在的文件,构建即碎。
  it("两个弹窗都串联 update-dialog-visibility", () => {
    for (const name of ["force-update-dialog.json", "update-progress-dialog.json"]) {
      expect(load(name).registryDependencies, name).toContain(
        "@swarmhive/update-dialog-visibility",
      );
    }
  });

  it("prompt-update-dialog 已 inline content 且串联 registryDependencies", () => {
    const item = load("prompt-update-dialog.json");
    expect(item.files?.[0].content?.length ?? 0).toBeGreaterThan(0);
    expect(item.registryDependencies).toContain("@swarmhive/use-update");
    expect(item.registryDependencies).toContain("dialog");
    expect(item.dependencies).toContain("lucide-react");
  });

  it("use-update 串联到 tauri-adapter", () => {
    expect(load("use-update.json").registryDependencies).toContain("@swarmhive/tauri-adapter");
  });

  it("download-panel 带上 SDK 与基础 UI 依赖", () => {
    const item = load("download-panel.json");
    expect(item.dependencies).toContain("@swarm-hive/sdk");
    expect(item.dependencies).toContain("lucide-react");
    expect(item.registryDependencies).toContain("button");
    expect(item.registryDependencies).toContain("utils");
  });
});
