# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 开发工作流

**IMPORTANT**：执行任何开发任务（编写代码、修改配置、添加依赖、推进 OpenSpec proposal）前，必须先调用 `/dev-workflow` skill。它会加载项目知识库（`dev-notes/knowledge/`）中的最佳实践和踩坑记录，并在开发完成后引导更新知识库。

知识库主题：

- [dev-notes/knowledge/architecture.md](dev-notes/knowledge/architecture.md) — 4 crate 边界、存储抽象、上传链路、部署形态、SDK/registry 分发
- [dev-notes/knowledge/backend.md](dev-notes/knowledge/backend.md) — sea-orm 2 entity、auth (argon2/session/PAT/Token)、storage trait、mailer、RFC 9457
- [dev-notes/knowledge/admin-spa.md](dev-notes/knowledge/admin-spa.md) — Vite 8 + React 19 + AntD 6 Pro + TanStack Router/Query + utoipa client
- [dev-notes/knowledge/toolchain.md](dev-notes/knowledge/toolchain.md) — Rust 2024/1.90、Cargo + pnpm 双 workspace、Biome、Lefthook、Conventional Commits、CI
- [dev-notes/knowledge/openspec-workflow.md](dev-notes/knowledge/openspec-workflow.md) — proposal 命名、依赖图、tasks/design 模板、`/opsx:*` 命令流

## Project

SwarmHive is a self-hosted update distribution hub for **Tauri desktop apps** and **React Native Android apps**. It is a sub-project of the swarm-apps family (github.com/swarm-apps/swarmhive). Authoritative product / architecture docs live under `docs/` — start with [docs/README.md](docs/README.md) and read [docs/03-architecture.md](docs/03-architecture.md) before non-trivial work.

## Repository layout

Polyglot monorepo with two workspace systems rooted at the repo root:

- **Cargo workspace** (`Cargo.toml`) → 4 crate
  - `swarmhive-api-types`: shared HTTP DTOs (serde + `utoipa::ToSchema`). **No** sea-orm / axum / tokio / reqwest. Consumed by server, CLI, and any future client.
  - `swarmhive-entity`: sea-orm Entity / ActiveModel + `From<&Model>` conversions to api-types. Server-side only.
  - `swarmhive-server`: Axum HTTP server. Has both `[lib]` (`swarmhive_server::*` — exposes `build_router(state)` for integration tests) and `[[bin]]` (`swarmhive-server` binary in `src/bin/server.rs`). Binds `0.0.0.0:3030`, will embed admin SPA via `rust-embed`.
  - `swarmhive-cli`: clap CLI, binary name is `swarmhive` (not `swarmhive-cli`). Must **not** depend on entity / sea-orm — only api-types.
- **pnpm workspace** (`pnpm-workspace.yaml`) → `apps/*`, `packages/*`
  - `apps/admin` (`@swarmhive/admin`): Vite + React 19 + TanStack Router/Query + AntD 6 + Pro Components. Dev server on `:5173` proxies `/api` and `/healthz` to the Rust server on `:3030`.
  - `packages/*` (sdk-core, tauri, react-native, registry-web, registry-rn) is reserved per architecture doc but not yet scaffolded.

The CLI binary, npm scope, and Rust crate share the `swarmhive` brand: CLI binary `swarmhive`, npm scope `@swarmhive/*`, Rust crates `swarmhive-*` (kebab) imported as `swarmhive_*` (underscore).

## Common commands

Run from the repo root unless noted.

```bash
# Admin SPA
pnpm admin:dev                              # vite dev on :5173 (proxies to server :3030)
pnpm admin:build                            # vite build → apps/admin/dist
pnpm --filter @swarmhive/admin typecheck    # tsc -b (router type generation must succeed)

# JS/TS lint + format (Biome)
pnpm lint                                   # biome check .
pnpm format                                 # biome check --write .
pnpm lint:ci                                # biome ci . (CI mode, no autofix)

# Local dev DB (Postgres 17 on :5433 to avoid clashing with any host 5432)
docker run -d --name swarmhive-pg \
  -e POSTGRES_USER=swarmhive -e POSTGRES_PASSWORD=swarmhive-dev -e POSTGRES_DB=swarmhive \
  -p 5433:5432 -v swarmhive-pg-data:/var/lib/postgresql/data \
  --restart unless-stopped postgres:17
# Subsequent runs: docker start swarmhive-pg

# Rust
cargo build --workspace                     # build all crates
cargo run -p swarmhive-server               # start server on :3030 — reads config/default.toml at cwd
                                            #   endpoints: /healthz, /api/v1/version,
                                            #              /api/v1/auth/{login,logout,me,cli-token},
                                            #              /api/v1/setup{,info},
                                            #              /api/v1/tokens (GET/POST), /api/v1/tokens/{id} (DELETE),
                                            #              /api/openapi.json, /api/docs (Redoc UI)
                                            #   env overrides: SWARMHIVE_<SECTION>__<KEY>=<value> (e.g. SWARMHIVE_DATABASE__URL)
                                            #   first run prints a one-shot setup token to stdout — POST it to /api/v1/setup
                                            #   with { token, email, display_name, password } to create the Owner (auto-login).
                                            #   To re-issue: truncate the `user` table and restart the server.
cargo run -p swarmhive-cli -- login         # interactive: prompts email + password (rpassword no-echo) →
                                            #   POST /api/v1/auth/cli-token → writes ~/.config/swarmhive/credentials.toml (0600)
                                            #   default server http://localhost:3030; pass URL to override.
                                            #   `SWARMHIVE_TOKEN` env beats the file when both are present.
cargo run -p swarmhive-cli -- logout        # revoke remote PAT (best-effort) + remove local credentials.
cargo run -p swarmhive-cli -- <subcommand>  # init/verify/publish/promote/rollback are still todo!() stubs.
cargo test --workspace                      # unit + integration (db_smoke / auth_smoke / bearer_smoke / cli_token_smoke / openapi_surface use testcontainers + Docker)
cargo fmt --all                             # required before commit (pre-commit hook runs --check)
cargo clippy --workspace --all-targets

# Changelog (conventional commits → CHANGELOG.md)
pnpm changelog                              # full git-cliff regen
pnpm changelog:latest                       # unreleased section only
```

## Tooling that gates commits

`lefthook.yml` wires two stages that **must pass**:

- **pre-commit** (parallel): `biome check` on staged JS/TS/JSON, `cargo fmt --check` on staged Rust. If `cargo fmt --check` fails, run `cargo fmt --all` and re-stage — do not bypass with `--no-verify`.
- **commit-msg**: `commitlint` enforces [Conventional Commits](https://www.conventionalcommits.org/). Commit subjects must match `type(scope): subject`. Used by `git-cliff` (`cliff.toml`) to generate the changelog.

`biome.json` ignores `target/`, `dist/`, `node_modules/`, `routeTree.gen.ts`, `pnpm-lock.yaml`, `CHANGELOG.md`. `routeTree.gen.ts` is generated by the TanStack Router Vite plugin at dev/build time — never hand-edit it.

## Architectural anchors

- **Server ↔ Admin coupling**: Admin SPA is built as a static bundle and embedded into the server binary via `rust-embed` (planned). Local dev does *not* embed — Vite serves the SPA on `:5173` and proxies API calls to the Rust server on `:3030`. Keep API paths under `/api/...` so the proxy and embedded fallback both work.
- **Storage abstraction**: Only one storage backend exists — S3-compatible (`aws-sdk-s3`). Single-server deployments use bundled RustFS reachable through the same S3 interface. There is no local-filesystem backend; do not add one. See [docs/07-storage-and-delivery.md](docs/07-storage-and-delivery.md).
- **Platform scope (MVP)**: Tauri full-app updater protocol and React Native Android APK updater only. Expo / CodePush-style OTA is deferred to a `provider` extension layer — do not bake OTA assumptions into core types. See [docs/04-platform-support.md](docs/04-platform-support.md) and [docs/11-ota-providers.md](docs/11-ota-providers.md).
- **CLI is the first-class publish path**, not the Web Admin. CI/CD reuses the same CLI binary via an official GitHub Action. Web Admin focuses on viewing, config, RBAC, storage wizard, and analytics. See [docs/12-cli.md](docs/12-cli.md), [docs/06-cicd.md](docs/06-cicd.md).
- **MVP is single-org + full RBAC**, not multi-tenant. Org / User / Role / Permission / UserRole entities exist but org boundary is deliberately one. See [docs/13-rbac.md](docs/13-rbac.md).
- **SDK is UI-less**: `@swarmhive/sdk-core` (+ `/react` sub-entry) exposes state machine + hooks; UI components are distributed via shadcn registries (`registry-web` for Tauri/Electron/Web, `registry-rn` for RN). Do not add UI components to the SDK packages. See [docs/14-sdk-ui.md](docs/14-sdk-ui.md).

## Conventions

- **Naming**: brand identifiers use the triple `SwarmHive` (display) / `swarmhive` (lowercase, paths, npm) / `SWARMHIVE` (env vars, e.g. `SWARMHIVE_TOKEN`).
- **Rust toolchain** tracks `channel = "stable"` via [rust-toolchain.toml](rust-toolchain.toml) with `edition = "2024"` and `rust-version = "1.94"` in [Cargo.toml](Cargo.toml). The 1.94 floor is above SeaORM 2.0's MSRV (1.85) and tolerates current `url`/`reqwest`/`aws-sdk-s3` indirect ICU 2.2 dependencies (1.86+). Do **not** lower MSRV without verifying ecosystem compatibility. See [dev-notes/knowledge/toolchain.md](dev-notes/knowledge/toolchain.md) for history.
- **Rust dependencies** are centralized in `[workspace.dependencies]` at the root `Cargo.toml`. Inside per-crate `Cargo.toml`, reference them via `<dep>.workspace = true`. Pin new shared deps at the workspace root.
- **Release profile** uses `lto = "thin"`, `codegen-units = 1`, `strip = true` — expect slow `--release` builds.
- **Server logs**: default `RUST_LOG`-style filter is `info,swarmhive_server=debug` (set via `EnvFilter::try_from_default_env`).

## Known environment quirks (Windows)

- Renamed Rust source directories may leave empty parent dirs locked by rust-analyzer / VSCode file watchers — non-fatal as long as no `Cargo.toml` is inside (the workspace ignores them). Close the watcher to clean up.
- Git `core.autocrlf` is on; expect `LF will be replaced by CRLF` warnings on add. Don't try to "fix" them.
