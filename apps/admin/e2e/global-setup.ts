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
const SERVER_ARGS = process.env.SWARMHIVE_E2E_BIN
  ? []
  : ["run", "-p", "swarmhive-server", "--quiet"];
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
