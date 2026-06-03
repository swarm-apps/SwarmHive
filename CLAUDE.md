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
  - `apps/admin` (`@swarm-hive/admin`): Vite + React 19 + TanStack Router/Query + AntD 6 + Pro Components. Dev server on `:5173` proxies `/api` and `/healthz` to the Rust server on `:3030`.
  - `packages/*` (sdk-core, tauri, react-native, registry-web, registry-rn) is reserved per architecture doc but not yet scaffolded.

The CLI binary, npm scope, and Rust crate share the `swarmhive` brand: CLI binary `swarmhive`, npm scope `@swarm-hive/*`, Rust crates `swarmhive-*` (kebab) imported as `swarmhive_*` (underscore).

## Common commands

Run from the repo root unless noted.

```bash
# Admin SPA
pnpm admin:dev                              # vite dev on :5173 (proxies to server :3030)
pnpm admin:build                            # vite build → apps/admin/dist
pnpm --filter @swarm-hive/admin typecheck    # tsc -b (router type generation must succeed)

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

# Local dev SMTP (mailpit — SMTP :1025 + Web UI :8025)
docker run -d --name swarmhive-mailpit \
  -p 1025:1025 -p 8025:8025 \
  --restart unless-stopped axllent/mailpit
# When [mail] seed_mailpit_in_dev = true (default in config/default.toml) and
# the mail_provider table is empty, the server seeds an active "mailpit" SMTP
# provider on first boot — emails sent by /test or invite/reset flows show up at
# http://localhost:8025.

# Local dev storage (bundled RustFS — S3 :9000 + console :9001)
# docker-compose.yml at repo root exposes a bundled-storage profile: it starts
# RustFS plus a one-shot init container that pre-creates the bucket (server
# probe/upload do NOT auto-create buckets — the bucket must already exist).
docker compose --profile bundled-storage up -d   # rustfs + bucket (default rustfsadmin/rustfsadmin, bucket swarmhive)
# Wire it in and activate (create → put/get/delete probe → hot-swap activate; force_path_style auto-true):
cargo run -p swarmhive-cli -- storage init rustfs \
  --bucket swarmhive --access-key-id rustfsadmin --access-key-secret rustfsadmin
# compose only manages storage — pg / mailpit stay on their own `docker run` above
# (compose must NOT reuse their container names, or `down --remove-orphans` deletes them + their volumes).
# Override default RustFS secrets via .env (see .env.example).

# Rust
cargo build --workspace                     # build all crates
cargo run -p swarmhive-server               # start server on :3030 — reads config/default.toml at cwd
                                            #   endpoints: /healthz, /api/v1/version,
                                            #              /api/v1/auth/{login,logout,me},
                                            #              /api/v1/auth/device/{code,token,lookup,approve,deny} (RFC 8628 CLI login),
                                            #              /api/v1/setup{,info},
                                            #              /api/v1/tokens (GET/POST), /api/v1/tokens/{id} (DELETE),
                                            #              /api/v1/mail/{providers,templates,logs,status} (GET/POST/PUT/DELETE),
                                            #              /api/openapi.json, /api/docs (Redoc UI)
                                            #   env overrides: SWARMHIVE_<SECTION>__<KEY>=<value> (e.g. SWARMHIVE_DATABASE__URL)
                                            #   SWARMHIVE_SECRET_KEY (base64, 32 bytes) — fail-fast at startup; used to
                                            #     AES-256-GCM encrypt mail provider passwords and (future) OAuth client_secrets.
                                            #     Generate via `openssl rand -base64 32`. May instead live under
                                            #     `[secret] key` of `config/local.toml` (gitignored).
                                            #   first run prints a banner pointing at /setup; the Admin SPA routes any
                                            #   visit to /setup while the user table is empty (Coolify-style bootstrap).
                                            #   POST /api/v1/setup with { email, display_name, password } creates the
                                            #   Owner (auto-login). Optional: set SWARMHIVE_BOOTSTRAP_OWNER_EMAIL=<email>
                                            #   to pin the owner email and reject any other claimant (recommended for
                                            #   public deployments). Password must be ≥12 chars, ≥3 character classes,
                                            #   and not in the bundled top-100 weak-password dictionary.
                                            #   To re-bootstrap: truncate the `user` table and restart the server.
cargo run -p swarmhive-cli -- login         # RFC 8628 device flow (no password): POST /api/v1/auth/device/code →
                                            #   prints a user_code + opens the browser to {base_url}/device; user
                                            #   approves in the web UI (password OR GitHub) → CLI polls
                                            #   /api/v1/auth/device/token → mints a PAT → writes
                                            #   ~/.config/swarmhive/credentials.toml (0600). OAuth-only users can log
                                            #   in the CLI too (auth happens in the browser, reusing /login).
                                            #   default server http://localhost:3030; pass URL to override.
                                            #   `SWARMHIVE_TOKEN` env beats the file when both are present.
cargo run -p swarmhive-cli -- logout        # revoke remote PAT (best-effort) + remove local credentials.
cargo run -p swarmhive-cli -- <subcommand>  # init/verify/publish/promote/rollback are still todo!() stubs.
cargo test --workspace                      # unit + integration (db_smoke / auth_smoke / bearer_smoke / device_login_smoke / bootstrap_smoke / login_lockout_smoke / openapi_surface use testcontainers + Docker)
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
- **SDK is UI-less**: `@swarm-hive/sdk-core` (+ `/react` sub-entry) exposes state machine + hooks; UI components are distributed via shadcn registries (`registry-web` for Tauri/Electron/Web, `registry-rn` for RN). Do not add UI components to the SDK packages. See [docs/14-sdk-ui.md](docs/14-sdk-ui.md).

## Conventions

- **Naming**: brand identifiers use the triple `SwarmHive` (display) / `swarmhive` (lowercase, paths, npm) / `SWARMHIVE` (env vars, e.g. `SWARMHIVE_TOKEN`).
- **代码注释用中文**:给开发者看的注释(`//`、`///`、`//!`)一律用中文。**面向用户的文案保持英文**——RFC 9457 error `detail`/`title`、OpenAPI `description`、clap `#[arg]`/subcommand 的 `--help`、`tracing` 的 action 名等;这些等单独的 i18n 决策再统一处理。存量英文注释碰到对应文件时顺手转中文,不为转注释单独改无关文件。截至 2026-05-29 已转的范围:storage/upload feature(server `routes/{storage,uploads,download}`、`storage/{mod,s3}`、`services/storage`、entity `storage_backend`/`upload_session`、api-types `storage`/`upload`、CLI `commands/{client,publish,verify,storage}` + `config`)。
- **Rust toolchain** tracks `channel = "stable"` via [rust-toolchain.toml](rust-toolchain.toml) with `edition = "2024"` and `rust-version = "1.94"` in [Cargo.toml](Cargo.toml). The 1.94 floor is above SeaORM 2.0's MSRV (1.85) and tolerates current `url`/`reqwest`/`aws-sdk-s3` indirect ICU 2.2 dependencies (1.86+). Do **not** lower MSRV without verifying ecosystem compatibility. See [dev-notes/knowledge/toolchain.md](dev-notes/knowledge/toolchain.md) for history.
- **Rust dependencies** are centralized in `[workspace.dependencies]` at the root `Cargo.toml`. Inside per-crate `Cargo.toml`, reference them via `<dep>.workspace = true`. Pin new shared deps at the workspace root.
- **Release profile** uses `lto = "thin"`, `codegen-units = 1`, `strip = true` — expect slow `--release` builds.
- **Server logs**: default `RUST_LOG`-style filter is `info,swarmhive_server=debug` (set via `EnvFilter::try_from_default_env`).
- **Mermaid diagrams in `docs/` / `dev-notes/`**: mermaid 用同一套 token 表达节点形状（`[` 矩形 / `{` 菱形 / `(` 圆角 / `[(` 圆柱 / `((` 圆形 / `[/` 梯形），节点文本 / 边 label 里出现这些字符会被优先按形状解析。规则：
  - **节点文本含 `[]` `()` `{}`** → 用引号包裹 `A["text[含括号]"]`，含 `<>` 用 HTML 实体 `&lt;` / `&gt;`。例：Rust 属性宏 `#[utoipa::path]`、TS 类型 `Result<T, E>`、JSON 字面量 `{ foo: 1 }` 一律包引号。
  - **flowchart 边 label `|...|` 含 `()` `{}` `[]`** → 引号包裹支持比节点弱，**优先改写避免**（把数据细节挪到节点文本里）；中文 / 空格在 flowchart 边 label 合法。
  - **erDiagram 关系标签 `:label`** → 只接受 ASCII alphanumeric，中文 / 标点全部非法（用 `references` 不用 `引用`）。
  - **sequenceDiagram message** → 相对宽松，`()` `{}` `[]` 都合法。
  - **节点文本以 `/` 开头或结尾**（如 `UI[/api/docs/]`）→ 触发梯形形状解析，路径要么引号包裹要么改写。
  - **根因**：mermaid 的形状 token 优先级高于文本内容；不确定时**节点文本一律加引号**是最稳的兜底。

## Known environment quirks (Windows)

- Renamed Rust source directories may leave empty parent dirs locked by rust-analyzer / VSCode file watchers — non-fatal as long as no `Cargo.toml` is inside (the workspace ignores them). Close the watcher to clean up.
- Git `core.autocrlf` is on; expect `LF will be replaced by CRLF` warnings on add. Don't try to "fix" them.
