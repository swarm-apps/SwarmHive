# swarmhive CLI — full command reference

Binary `swarmhive` (dev: `cargo run -p swarmhive-cli --`). All commands accept the **global**
`--output {table|json}` (default `table`). With `--output json`: success → JSON on stdout, failure →
RFC 9457 problem+json on stderr, non-zero exit. Destructive verbs require `--yes`.

## Table of contents
- [Auth & environment](#auth--environment)
- [init](#init)
- [tokens](#tokens)
- [verify](#verify)
- [publish](#publish)
- [register](#register)
- [apps](#apps)
- [channels](#channels)
- [releases](#releases)
- [artifacts](#artifacts)
- [storage](#storage)
- [mail](#mail)
- [notifications](#notifications)
- [telemetry](#telemetry)
- [login / logout / version](#login--logout--version)

## Auth & environment
- `SWARMHIVE_TOKEN` — bearer PAT (beats the credentials file). Required for agent/CI use.
- `SWARMHIVE_SERVER` — server URL override (else `swarmhive.toml`'s `server`, else credentials).
- `SWARMHIVE_CA_CERT` / `--ca-cert <pem>` — extra root CA for self-signed/enterprise TLS.
- `SWARMHIVE_STORAGE_SECRET` — S3 access-key secret for `storage`.
- `SWARMHIVE_MAIL_PASSWORD` — SMTP password for `mail`.
- `SWARMHIVE_WEBHOOK_SECRET` — IM webhook signing key for `notifications endpoints create`.
- Secret precedence everywhere: `--secret-stdin` (pipe) > env > plaintext flag. Omit on update = keep existing.

## init
`swarmhive init [flags]` — scaffold `swarmhive.toml`. Dual-mode: TTY+no-`--yes` prompts; `--yes` or
non-TTY is flag-driven (no prompts). Flags override prompts/defaults. Local-only.
- `--app <slug>` (default: cwd name) · `--server <url>` · `--platform <tauri|android>` (repeatable)
- `--tauri-conf <path>` (default `src-tauri/tauri.conf.json`) · `--android-apk <path>` (default `app/build/outputs/apk/release/app-release.apk`)
- `--yes` (non-interactive) · `--force` (overwrite existing file)
- `--setup-ci-token` — also write `.github/workflows/release.yml` (swarmhive-action@v2; upload→draft→finalize) and emit token/secret commands (offline; no API call). JSON adds `suggested_token_command`, `github_secret_name`, `suggested_secret_command`, `suggested_workflow_path`, `workflow_created`.
- JSON success: `{ path, app, server?, platforms[], created }`. `artifacts` is emitted as a commented stub — user fills real paths.

## tokens
`swarmhive tokens <list|create|delete>` — manage scoped API tokens / PATs for CI.
- `list`
- `create --name <n> [--kind pat|api] [--permissions <a,b,c>] [--preset ci-publish]` — `api` (default) needs `--permissions` **or** `--preset`; `pat` inherits the owner's live perms. `--preset ci-publish` expands to `app:read,release:read,release:create,release:update,release:publish,release:promote,artifact:upload` (the full publish set incl. `release:update`); mutually exclusive with `--permissions`. Plaintext token shown **once** on create.
- `delete --id <id> --yes`

## verify
Preflight, no upload. `swarmhive verify tauri|android [flags]`.
- common: `--app <slug>` · `--dry-run` (skip server duplicate check) · `--ca-cert <pem>` · global `--output`
- `tauri`: `--version <v>` (else read tauri.conf.json) · `--conf <path>` · `--artifact <path>` (repeatable; else config)
- `android`: `--version <v>` (req) · `--version-code <i64>` (req) · `--apk <path>` (else config)
- Checks artifact existence + sha256, parses `latest.json` (tauri), warns if the server already has the version (unless `--dry-run`).

## publish
Upload artifacts to a **draft** release (does NOT publish by default — `harden-publish-flow`).
`swarmhive publish tauri|android [flags]`. Flow: ensure draft → presign → stream-upload (progress bar;
hidden under `--output json`/non-TTY) → complete (release stays `draft`) → conditional notes PATCH →
optional finalize.
- common (`CommonArgs`): `--app <slug>` · `--finalize` (publish after uploading; omit → leave draft) ·
  `--channel <name>` (point channel at the release; **implies --finalize**) ·
  `--notes-file <path>` / `--notes <text>` (changelog; file wins; PATCHed only when changed and after
  upload) · `--skip-notes-update` (never PATCH notes) · `--dry-run` (local plan only — zero network, no
  creds) · `--artifact <path>` (repeatable) · `--ca-cert <pem>`. **`--no-publish` removed** (draft is the default).
- `--mirror-url <url>` — record a GitHub Release asset URL as a **mirror / fallback download source** for
  this artifact (verbatim; the server allowlists it to `github.com` + the app's configured repo). **Single-artifact
  publishes only** (mirror URL is per-asset); errors if >1 artifact is planned. CI passes the deterministic URL it
  uploads to GitHub Releases in the same run. Requires server ≥ 0.7.0 (`add-github-release-source`).
- `tauri`: `--version <v>` (else tauri.conf.json) · `--target <triple>` (e.g. x86_64-pc-windows-msvc) · `--conf <path>`
- `android`: `--version <v>` (req) · `--version-code <i64>` (req) · `--apk <path>` · `--abi <abi>` (e.g. arm64-v8a; omit → fat APK)
- A sibling `<artifact>.sig` (minisign) is auto-uploaded for the Tauri updater. Per-ABI split APKs: run `publish android` once per ABI (the draft ensure is idempotent).
- **Multi-target**: upload each target (no `--finalize`) → one `releases finalize` at the end.
- JSON success: `{ app, version, status, published, channel?, artifacts[{filename,size,sha256,signed}], endpoints{platform:url} }` (`status` is `draft` unless `--finalize`/`--channel`).

## register
Register an artifact whose bytes live **only on a GitHub Release** (no S3 upload) — for GitHub-as-a-download-source
without an object-storage backend, or to attach a mirror when the bytes aren't uploaded through SwarmHive.
`swarmhive register tauri|android [flags]` (`add-github-release-source`). Flow: ensure draft → hash the local file
(sha256 + size; **bytes are NOT uploaded, only hashed**) → find sibling `.sig` → `POST .../uploads/register` →
conditional notes PATCH → optional finalize. The server records `mirror_url` with no `object_key`.
- `--mirror-url <url>` **(required)** — the GitHub Release asset URL (allowlisted to `github.com` + the app's repo).
- Shares `publish`'s common flags: `--app` · `--finalize` · `--channel` (implies finalize) · `--notes-file`/`--notes`/
  `--skip-notes-update` · `--dry-run` (hash locally, zero network) · `--ca-cert` · `--output`.
- `tauri`: `--artifact <path>` · `--version <v>` (else tauri.conf.json) · `--target <triple>` · `--conf <path>`.
- `android`: `--apk <path>` (else `swarmhive.toml` `[app.android].apk`) · `--version <v>` (req) ·
  `--version-code <i64>` (req) · `--abi <abi>` · `--signature-file <path>`.
- **One artifact per invocation** (mirror URL is per-asset); multi-asset GitHub releases = N `register` + one `releases finalize`.
- The recorded mirror is only served after the server verifies the GitHub asset is publicly reachable and its digest
  matches the artifact's sha256 (draft window / drift are gated automatically).

## apps
`swarmhive apps <list|get|create|update|delete>`
- `list` · `get --app <slug>`
- `create --slug <s> --display-name <n> --platforms <tauri-desktop,react-native-android>` (comma list)
- `update --app <slug> [--display-name <n>] [--platforms <list>]` (slug immutable)
- `delete --app <slug> --yes` (fails if the app still has releases)

## channels
`swarmhive channels <list|create|set-default|promote|rollback>` — all `--app <slug>`.
- `list --app <s>` · `create --app <s> --name <c>` · `set-default --app <s> --name <c>`
- `promote --app <s> --name <c> --version <v>` (point channel at a published version)
- `rollback --app <s> --name <c> [--to-version <v>]` (default: previous distinct release)

## releases
`swarmhive releases <list|get|create|update|publish|finalize|yank>` — all `--app <slug>`.
Note: `releases publish`/`finalize` publish an existing **draft**; NOT the upload-style `publish tauri|android`.
- `list --app <s>` · `get --app <s> --version <v>`
- `create --app <s> --version <v> [--android-version-code <i64>] [--android-min-version-code <i64>] [--notes-file <path>]` (makes a draft, no upload)
- `update --app <s> --version <v> [--android-version-code <i64>] [--rollout-percent <1..100>] [--min-version <semver>] [--android-min-version-code <i64>] [--notes-file <path>]`
- `publish --app <s> --version <v>` (draft → published) · `finalize --app <s> --version <v>` (idempotent publish; validates ≥1 artifact; the recommended multi-target finish) · `yank --app <s> --version <v> --yes`
- Policy flags on `update`: `--rollout-percent 100` disables gray rollout; `--min-version 0.0.0` removes the Tauri force-update floor; omitted flags leave existing values unchanged. `--android-min-version-code` sets the RN Android force-update floor (versionCode).

## artifacts
`swarmhive artifacts list --app <slug> --version <v>` — read-only listing (filename/platform/target/abi/size/sha256).

## storage
`swarmhive storage <init|list|get|create|update|test|activate|cors>` — needs `storage:manage`.
- `init rustfs --bucket <b> --access-key-id <id> --access-key-secret <secret> [--name rustfs] [--endpoint http://localhost:9000] [--region us-east-1] [--public-bucket] [--public-base-url <url>] [--ca-cert <pem>]` — guided: create → put/get/delete probe → activate; `force_path_style` is forced true.
- `list` · `get --backend <id|name>` · `test --backend <x>` · `activate --backend <x>` (hot-swaps the active handle)
- `create --name <n> --endpoint <url> --bucket <b> --access-key-id <id> [--access-key-secret <s> | --secret-stdin | env SWARMHIVE_STORAGE_SECRET] [--region us-east-1] [--force-path-style] [--prefix <p>] [--public-base-url <url>] [--url-mode signed|public] [--signed-url-ttl-secs 600]`
- `update --backend <x> [any of the create fields]` (secret omitted = unchanged)
- `cors --backend <x> --origin <url>` (repeat `--origin`) — allow browser direct uploads

## mail
`swarmhive mail <providers|templates|logs|status>` — needs mail-manage permission.
- `providers list`
- `providers create --name <n> --host <h> --port <p> --from-email <e> [--encryption starttls|tls|none] [--from-name <n>] [--reply-to <e>] [--username <u>] [--password <p> | --secret-stdin | env SWARMHIVE_MAIL_PASSWORD]`
- `providers update --provider <id|name> [any create field]` (password omitted = unchanged)
- `providers activate --provider <x>` · `providers delete --provider <x> --yes` · `providers test --provider <x>` (sends self-test to the authenticated user)
- `templates list` · `templates get --event <e> --locale <l>`
- `templates set --event <e> --locale <l> [--subject <s>] [--html-file <path>] [--text-file <path>]` (omitted fields unchanged)
- `templates preview --event <e> --locale <l> --sample-file <json>` (renders with a minijinja sample context)
- `templates restore-defaults`
- `logs [--limit 50]` · `status` (active transport + fallback flag)

## notifications
`swarmhive notifications <endpoints|subscriptions|deliveries>` — needs `notification:manage`.

Endpoints:
- `endpoints list`
- `endpoints create --name <n> --url <url> [--provider generic|feishu|slack|dingtalk|discord] [--secret <s> | --secret-stdin | env SWARMHIVE_WEBHOOK_SECRET]`
  - `generic` returns a generated `whsec_` secret exactly once.
  - IM providers use the supplied signing key where applicable; do not pass secrets in plaintext unless the user accepts shell-history/ps exposure.
- `endpoints update --endpoint <id|name> [--name <n>] [--url <url>] [--disable | --enable]`
- `endpoints delete --endpoint <id|name> --yes`
- `endpoints rotate-secret --endpoint <id|name>` — generic only; returns the new `whsec_` exactly once, old secret remains valid for 24h dual-signing; rotating again during the grace window returns 409.
- `endpoints test --endpoint <id|name>` — sends a signed `webhook.test`; not written to the delivery log.

Subscriptions:
- `subscriptions list`
- `subscriptions create --event <release.published|channel.promoted|channel.rolled_back> --channel <email|webhook> [--to <email>] [--endpoint <id|name>] [--app <slug>]`
  - `--channel email` requires `--to` (and rejects `--endpoint`).
  - `--channel webhook` requires `--endpoint` (and rejects `--to`).
  - Omit `--app` to match all apps.
- `subscriptions delete --id <uuid> --yes`

Deliveries:
- `deliveries list [--endpoint <id|name>] [--status pending|sent|failed|dead] [--limit 50]`
- `deliveries get --id <uuid>` — table clips bodies and shows attempt count; `--output json` includes full snapshots and attempts.
- `deliveries redeliver --id <uuid>` — re-enqueues the delivery and preserves the original `webhook-id`.

## telemetry
`swarmhive telemetry <overview|summary|adoption|funnel|distribution>` — needs `telemetry:read`.

- `overview [--days 30]` — cross-app home dashboard overview: app count, release count, update checks, downloads completed, trend.
- `summary --app <slug> [--days 30]` — per-app metric cards (today active devices, downloads, latest version).
- `adoption --app <slug> [--days 30]` — per-app version adoption (daily unique devices; `version=null` is the daily total row).
- `funnel --app <slug> [--days 30]` — occurrence-based update funnel, not device-deduplicated.
- `distribution --app <slug> [--days 30] [--dim platform]` — update-check distribution. `--dim` accepts `platform`, `arch`, `version`, or `channel`.

## login / logout / version
- `swarmhive login [server]` — **interactive** RFC 8628 device flow for humans (opens browser). Don't drive this as an agent; use `SWARMHIVE_TOKEN` instead. Default server `http://localhost:3030`.
- `swarmhive logout` — revoke the remote PAT (best-effort) + remove the local credentials file.
- `swarmhive version` — print CLI version.

## Object path model (context, not a flag)
Releases are immutable and addressed by version; channels are moving pointers. `promote`/`rollback`
only move the channel pointer — artifacts are never re-uploaded or deleted. Object key shape:
`{prefix}/apps/{slug}/versions/{version}/{platform}/{target|abi}/{filename}`.
