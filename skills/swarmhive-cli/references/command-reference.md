# swarmhive CLI — full command reference

Binary `swarmhive` (dev: `cargo run -p swarmhive-cli --`). All commands accept the **global**
`--output {table|json}` (default `table`). With `--output json`: success → JSON on stdout, failure →
RFC 9457 problem+json on stderr, non-zero exit. Destructive verbs require `--yes`.

## Table of contents
- [Auth & environment](#auth--environment)
- [init](#init)
- [verify](#verify)
- [publish](#publish)
- [apps](#apps)
- [channels](#channels)
- [releases](#releases)
- [artifacts](#artifacts)
- [storage](#storage)
- [mail](#mail)
- [login / logout / version](#login--logout--version)

## Auth & environment
- `SWARMHIVE_TOKEN` — bearer PAT (beats the credentials file). Required for agent/CI use.
- `SWARMHIVE_SERVER` — server URL override (else `swarmhive.toml`'s `server`, else credentials).
- `SWARMHIVE_CA_CERT` / `--ca-cert <pem>` — extra root CA for self-signed/enterprise TLS.
- `SWARMHIVE_STORAGE_SECRET` — S3 access-key secret for `storage`.
- `SWARMHIVE_MAIL_PASSWORD` — SMTP password for `mail`.
- Secret precedence everywhere: `--secret-stdin` (pipe) > env > plaintext flag. Omit on update = keep existing.

## init
`swarmhive init [flags]` — scaffold `swarmhive.toml`. Dual-mode: TTY+no-`--yes` prompts; `--yes` or
non-TTY is flag-driven (no prompts). Flags override prompts/defaults. Local-only.
- `--app <slug>` (default: cwd name) · `--server <url>` · `--platform <tauri|android>` (repeatable)
- `--tauri-conf <path>` (default `src-tauri/tauri.conf.json`) · `--android-apk <path>` (default `app/build/outputs/apk/release/app-release.apk`)
- `--yes` (non-interactive) · `--force` (overwrite existing file)
- JSON success: `{ path, app, server?, platforms[], created }`. `artifacts` is emitted as a commented stub — user fills real paths.

## verify
Preflight, no upload. `swarmhive verify tauri|android [flags]`.
- common: `--app <slug>` · `--dry-run` (skip server duplicate check) · `--ca-cert <pem>` · global `--output`
- `tauri`: `--version <v>` (else read tauri.conf.json) · `--conf <path>` · `--artifact <path>` (repeatable; else config)
- `android`: `--version <v>` (req) · `--version-code <i64>` (req) · `--apk <path>` (else config)
- Checks artifact existence + sha256, parses `latest.json` (tauri), warns if the server already has the version (unless `--dry-run`).

## publish
Upload + create (and by default publish) a release. `swarmhive publish tauri|android [flags]`.
Flow: ensure draft → presign → stream-upload (progress bar; hidden under `--output json`/non-TTY) → complete.
- common (`CommonArgs`): `--app <slug>` · `--channel <name>` (promote this channel after publish) ·
  `--no-publish` (upload but leave draft) · `--notes-file <path>` / `--notes <text>` (changelog; file wins) ·
  `--dry-run` (local plan only — zero network, no creds) · `--artifact <path>` (repeatable) · `--ca-cert <pem>`
- `tauri`: `--version <v>` (else tauri.conf.json) · `--target <triple>` (e.g. x86_64-pc-windows-msvc) · `--conf <path>`
- `android`: `--version <v>` (req) · `--version-code <i64>` (req) · `--apk <path>` · `--abi <abi>` (e.g. arm64-v8a; omit → fat APK)
- A sibling `<artifact>.sig` (minisign) is auto-uploaded for the Tauri updater. Per-ABI split APKs: run `publish android` once per ABI (the draft ensure is idempotent).
- JSON success: `{ app, version, status, published, channel?, artifacts[{filename,size,sha256,signed}], endpoints{platform:url} }`.

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
`swarmhive releases <list|get|create|update|publish|yank>` — all `--app <slug>`.
Note: `releases publish` publishes an existing **draft**; it is NOT the upload-style `publish tauri|android`.
- `list --app <s>` · `get --app <s> --version <v>`
- `create --app <s> --version <v> [--android-version-code <i64>] [--notes-file <path>]` (makes a draft, no upload)
- `update --app <s> --version <v> [--android-version-code <i64>] [--notes-file <path>]`
- `publish --app <s> --version <v>` · `yank --app <s> --version <v> --yes`

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

## login / logout / version
- `swarmhive login [server]` — **interactive** RFC 8628 device flow for humans (opens browser). Don't drive this as an agent; use `SWARMHIVE_TOKEN` instead. Default server `http://localhost:3030`.
- `swarmhive logout` — revoke the remote PAT (best-effort) + remove the local credentials file.
- `swarmhive version` — print CLI version.

## Object path model (context, not a flag)
Releases are immutable and addressed by version; channels are moving pointers. `promote`/`rollback`
only move the channel pointer — artifacts are never re-uploaded or deleted. Object key shape:
`{prefix}/apps/{slug}/versions/{version}/{platform}/{target|abi}/{filename}`.
