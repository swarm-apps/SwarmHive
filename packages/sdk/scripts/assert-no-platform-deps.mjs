// 守护:@swarm-hive/sdk 必须零平台依赖(同 CLI 的 `cargo tree | grep sea-orm` 范式)。
// 平台 adapter(Tauri/RN)在 registry,不在 npm 包。`react` 是 optional peer,允许。

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const pkg = JSON.parse(readFileSync(join(here, "..", "package.json"), "utf8"));
const deps = Object.keys(pkg.dependencies ?? {});

const forbidden = deps.filter(
  (d) => d.startsWith("@tauri-apps/") || d.startsWith("expo") || d === "react-native",
);

if (forbidden.length > 0) {
  console.error(
    `✗ @swarm-hive/sdk must stay platform-free, but dependencies include: ${forbidden.join(", ")}`,
  );
  console.error("  Move platform adapters into registry-web / registry-rn instead.");
  process.exit(1);
}

console.log("✓ @swarm-hive/sdk has zero platform dependencies");
