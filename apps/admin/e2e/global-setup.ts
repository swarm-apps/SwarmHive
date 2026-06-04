import { spawn } from "node:child_process";
import type { StartedPostgreSqlContainer } from "@testcontainers/postgresql";
import { PostgreSqlContainer } from "@testcontainers/postgresql";

declare global {
  // eslint-disable-next-line no-var
  var __SWARMHIVE_E2E__: {
    container?: StartedPostgreSqlContainer;
    serverPid: number;
  };
}

const SERVER_HOST = process.env.SWARMHIVE_E2E_HOST ?? "127.0.0.1";
const SERVER_PORT = Number(process.env.SWARMHIVE_E2E_PORT ?? 3030);
const SERVER_BIN = process.env.SWARMHIVE_E2E_BIN ?? "cargo";
// 本地无预构建 BIN 时走 cargo run；crate 有两个 bin(swarmhive-server + dump-openapi),
// 必须 --bin 指定,否则 "could not determine which binary to run"。
const SERVER_ARGS = process.env.SWARMHIVE_E2E_BIN
  ? []
  : ["run", "-p", "swarmhive-server", "--bin", "swarmhive-server", "--quiet"];
// server bin 启动 eager fail-fast 要求 SWARMHIVE_SECRET_KEY(crypto::SecretKey::
// from_env_or_config)。CI 既不设该 env、又无 config/local.toml(gitignored) → 不给
// server 起不来、/healthz 永不就绪 → 90s 超时(长期红的真因)。E2E 是临时库无持久密文,
// 固定测试 key 即可;真实 env 设了则优先。
const E2E_SECRET_KEY =
  process.env.SWARMHIVE_SECRET_KEY ?? "5T/dmJ969ylfCrrVtV9HJ8zTXPVZYS98lGfQ/dhGUFY=";
// When CI provides a Postgres service via `services:`, skip starting a
// testcontainer (and re-using the host-network Postgres at this URL).
const EXTERNAL_DATABASE_URL = process.env.SWARMHIVE_E2E_DATABASE_URL;

async function waitForHealth(url: string, deadlineMs: number): Promise<void> {
  const stop = Date.now() + deadlineMs;
  while (Date.now() < stop) {
    try {
      const res = await fetch(url);
      if (res.ok) return;
    } catch {
      // server not up yet
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`server did not become healthy at ${url} within ${deadlineMs}ms`);
}

export default async function globalSetup(): Promise<void> {
  let container: StartedPostgreSqlContainer | undefined;
  let databaseUrl: string;

  if (EXTERNAL_DATABASE_URL) {
    databaseUrl = EXTERNAL_DATABASE_URL;
  } else {
    container = await new PostgreSqlContainer("postgres:17")
      .withDatabase("swarmhive")
      .withUsername("swarmhive")
      .withPassword("swarmhive-dev")
      .start();
    databaseUrl = container.getConnectionUri();
  }

  const child = spawn(SERVER_BIN, SERVER_ARGS, {
    cwd: process.cwd().replace(/apps[\\/]admin.*$/, ""),
    env: {
      ...process.env,
      SWARMHIVE_DATABASE__URL: databaseUrl,
      SWARMHIVE_SERVER__HOST: SERVER_HOST,
      SWARMHIVE_SERVER__PORT: String(SERVER_PORT),
      SWARMHIVE_SECRET_KEY: E2E_SECRET_KEY,
      RUST_LOG: "warn",
    },
    stdio: "inherit",
  });

  await waitForHealth(`http://${SERVER_HOST}:${SERVER_PORT}/healthz`, 90_000);

  globalThis.__SWARMHIVE_E2E__ = {
    container,
    serverPid: child.pid ?? -1,
  };
}
