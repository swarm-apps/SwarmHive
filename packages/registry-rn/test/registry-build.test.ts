// registry build 产物校验 —— 断言 public/r 的 JSON 已 inline content + registryDependencies 链正确,
// 且【绝不】引 web Radix(dialog/button/progress)。这些文件随仓库提交(GitHub raw 分发的就是它们)。

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

describe("registry-rn build output", () => {
  it("registry.json 索引 9 个 item", () => {
    expect(load("registry.json").items).toHaveLength(9);
  });

  it("rn-adapter 已 inline content 且 deps 含 SDK", () => {
    const item = load("rn-adapter.json");
    expect(item.files?.[0].content?.length ?? 0).toBeGreaterThan(0);
    expect(item.dependencies).toContain("@swarm-hive/sdk");
  });

  it("prompt-update-dialog 串联到 use-update 且【无】web Radix 依赖", () => {
    const item = load("prompt-update-dialog.json");
    expect(item.files?.[0].content?.length ?? 0).toBeGreaterThan(0);
    expect(item.registryDependencies).toContain("@swarmhive-rn/use-update");
    // RN 用纯原语,绝不引 web 的 dialog/button/progress(会被 shadcn 解析成 Radix)。
    for (const web of ["dialog", "button", "progress"]) {
      expect(item.registryDependencies ?? []).not.toContain(web);
    }
  });

  it("use-update 串联到 rn-adapter", () => {
    expect(load("use-update.json").registryDependencies).toContain("@swarmhive-rn/rn-adapter");
  });

  it("全部 item 的 registryDependencies 仅用 @swarmhive-rn namespace,无 web Radix", () => {
    const webRadix = new Set(["dialog", "button", "progress"]);
    for (const item of load("registry.json").items ?? []) {
      for (const dep of item.registryDependencies ?? []) {
        expect(webRadix.has(dep)).toBe(false);
        if (dep.startsWith("@")) expect(dep.startsWith("@swarmhive-rn/")).toBe(true);
      }
    }
  });
});
