// registry build 产物校验 —— 断言 public/r 的 JSON 已 inline content + registryDependencies 链正确:
// UI 组件依赖【React Native Reusables 官方 registry】(@react-native-reusables/* namespace)拿
// button/dialog/alert-dialog/progress/text,而【绝不】引裸名 dialog/button/progress(那会被 shadcn
// 解析成 web 的 @shadcn/Radix 版本)。这些文件随仓库提交(GitHub raw 分发的就是它们)。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const rDir = fileURLToPath(new URL("../public/r/", import.meta.url));

function load(name: string): {
  files?: { content?: string }[];
  items?: { name: string; registryDependencies?: string[] }[];
  dependencies?: string[];
  registryDependencies?: string[];
} {
  return JSON.parse(readFileSync(`${rDir}${name}`, "utf8"));
}

// 裸名会被 shadcn 解析成 web 的 @shadcn 默认 registry(Radix)——RN 必须走 RNR namespace。
const WEB_RADIX = new Set(["dialog", "alert-dialog", "button", "progress", "text"]);

describe("registry-rn build output", () => {
  it("registry.json 索引 10 个 item", () => {
    expect(load("registry.json").items).toHaveLength(10);
  });

  it("rn-adapter 已 inline content 且 deps 含 SDK", () => {
    const item = load("rn-adapter.json");
    expect(item.files?.[0].content?.length ?? 0).toBeGreaterThan(0);
    expect(item.dependencies).toContain("@swarm-hive/sdk");
  });

  it("prompt-update-dialog 串联到 use-update + RNR Dialog,且无裸名 web Radix 依赖", () => {
    const item = load("prompt-update-dialog.json");
    expect(item.files?.[0].content?.length ?? 0).toBeGreaterThan(0);
    expect(item.registryDependencies).toContain("@swarmhive-rn/use-update");
    // UI 原语走 RNR 官方 registry 的 namespace 引用,不写裸名。
    expect(item.registryDependencies).toContain("@react-native-reusables/dialog");
    for (const bare of WEB_RADIX) {
      expect(item.registryDependencies ?? []).not.toContain(bare);
    }
  });

  it("force / progress 弹窗走 RNR AlertDialog", () => {
    for (const name of ["force-update-dialog.json", "update-progress-dialog.json"]) {
      const item = load(name);
      expect(item.registryDependencies).toContain("@react-native-reusables/alert-dialog");
    }
  });

  // 两个弹窗靠共享的 visibility 策略互斥(强制流只由 force 弹窗承载进度)。漏了这条依赖,
  // 下游拉取到的组件会 import 一个不存在的文件——分发即坏。
  it("force / progress 弹窗把 update-dialog-visibility 带进分发链", () => {
    for (const name of ["force-update-dialog.json", "update-progress-dialog.json"]) {
      expect(load(name).registryDependencies, name).toContain(
        "@swarmhive-rn/update-dialog-visibility",
      );
    }
    expect(load("update-dialog-visibility.json").files?.[0].content?.length ?? 0).toBeGreaterThan(
      0,
    );
  });

  it("use-update 串联到 rn-adapter", () => {
    expect(load("use-update.json").registryDependencies).toContain("@swarmhive-rn/rn-adapter");
  });

  it("全部 item 的 registryDependencies 只用 @swarmhive-rn / @react-native-reusables namespace,无裸名 web Radix", () => {
    for (const item of load("registry.json").items ?? []) {
      for (const dep of item.registryDependencies ?? []) {
        // 裸名(无 @ 前缀)只允许通用的 utils;其余裸名(dialog/button/...)一律禁止。
        if (!dep.startsWith("@")) {
          expect(dep).toBe("utils");
          continue;
        }
        const ok = dep.startsWith("@swarmhive-rn/") || dep.startsWith("@react-native-reusables/");
        expect(ok).toBe(true);
      }
    }
  });
});
