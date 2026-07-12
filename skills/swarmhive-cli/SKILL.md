---
name: swarmhive-cli
description: >-
  Drive the SwarmHive `swarmhive` CLI to publish and manage app update releases. Use this skill
  WHENEVER the user wants to publish / ship / release a Tauri desktop or React Native Android app
  update to a SwarmHive server, scaffold `swarmhive.toml`, run `swarmhive init / verify / publish`,
  promote or roll back a release channel (e.g. beta → stable), adjust rollout / force-update policy,
  query telemetry, manage SwarmHive storage backends, mail providers, or notification webhooks /
  subscriptions / delivery logs, or wire SwarmHive publishing into CI/CD — even if they only say "swarmhive",
  "publish a release", "push an update", "ship the new version", "promote to stable", "roll back the
  release", or name a swarm-apps product (SwarmDrop, SwarmNote, SwarmNote-RN). It encodes the CLI's
  non-interactive AI contract (SWARMHIVE_TOKEN env, `--output json`, `--yes`, `--secret-stdin`,
  `--dry-run`) so releases are driven safely and correctly. Prefer this over guessing `swarmhive`
  flags from memory.
---

# Driving the SwarmHive CLI

The `swarmhive` CLI is the first-class way to publish app updates to a self-hosted SwarmHive server
and to manage that deployment. It was built to be driven unattended by an agent: every command is
non-interactive, takes its inputs as flags, and speaks a stable JSON contract. Your job is to map
the user's intent to the right command(s), run them honestly, and parse the results — **not** to
guess flags or hand-write HTTP calls.

The binary is `swarmhive` (Rust crate `swarmhive-cli`). If it isn't on PATH in a dev checkout, run
it via `cargo run -p swarmhive-cli -- <args>`.

## Pick the command

- Ship built artifacts (installer / APK) to the server → `publish tauri` / `publish android` (add
  `--mirror-url <github-release-asset-url>` to also record a GitHub Release mirror / fallback source).
- Register an artifact hosted **only** on a GitHub Release (no S3 upload) → `register tauri|android --mirror-url <url>`.
- Configure an app's GitHub Release download source (the per-app allowlist gate `--mirror-url` is checked against) →
  `source set --app <slug> --owner <o> --repo <r>` (once per app); inspect with `source get`, remove with `source delete --yes`.
- Just move a channel pointer (beta → stable, or undo) → `channels promote` / `channels rollback`.
- Set up a repo to publish → `swarmhive init` (writes `swarmhive.toml`).
- Look before you leap → `verify tauri|android` (artifact check + server duplicate check) or `publish … --dry-run` (pure local plan).
- Inspect state → `apps list`, `releases list`, `channels list`, `artifacts list`.
- Adjust release policy → `releases update --rollout-percent ... --min-version ... --android-min-version-code ...`.
- Server admin (storage backend, SMTP, notifications, telemetry) → `storage …` / `mail …` /
  `notifications …` / `telemetry …` (see the reference).

`releases {create,update,publish}` manage release **metadata / drafts** — they do NOT upload
artifacts. The upload-and-publish flow is `publish tauri|android`. Don't conflate the two.

## The AI contract — internalize this first

These properties hold for **every** command and are why you can drive the CLI confidently:

- **Auth is ambient, never a prompt.** A bearer token comes from `SWARMHIVE_TOKEN` (and optional
  `SWARMHIVE_SERVER`) in the environment, beating the `swarmhive login` credentials file. For CI /
  agent use, assume `SWARMHIVE_TOKEN` is set; don't run `swarmhive login` (that's an interactive
  device-flow for humans). If a command fails with an auth error, tell the user to set
  `SWARMHIVE_TOKEN` — don't try to log in for them.
- **`--output json` is the parsing contract.** Add `--output json` to any command and: **success →
  one JSON object/array on stdout**; **failure → an RFC 9457 problem+json object on stderr**; and a
  **tiered exit code** (`harden-publish-flow`): `0` ok; `2` permanent (permission/config/validation —
  401/403/409/422 + local errors; retrying is pointless); `1` retryable (5xx/408/429, network timeout).
  In CI, branch on the code — re-run on `1`, fail fast on `2`. A `403` problem carries `remediation_hint`
  (also emitted as a GitHub Actions `::error::`). Parse stdout for results, read stderr's `detail` for
  the error reason. Default (no flag) is human-readable tables.
- **Destructive ops require `--yes`.** `apps delete`, `releases yank`, `mail providers delete` refuse
  to run without `--yes` — there is no interactive confirmation. This is your safety interlock (see
  Safety below).
- **Secrets never go in plaintext flags.** For S3 / SMTP secrets prefer `--secret-stdin` (pipe the
  value) or the env vars `SWARMHIVE_STORAGE_SECRET` / `SWARMHIVE_MAIL_PASSWORD` /
  `SWARMHIVE_WEBHOOK_SECRET`. A plaintext `--access-key-secret` / `--password` / `--secret` flag
  leaks into shell history and `ps` — only use it if the user explicitly accepts that.
- **`--dry-run` previews without uploading.** `publish` with `--dry-run` plans locally (locate
  artifacts, hash them, find `.sig`) and contacts nothing; `verify` checks artifacts and queries the
  server for a duplicate version. Use them before a real publish.

## swarmhive.toml — the project config

A consumer repo describes the app it publishes in a `swarmhive.toml` at the project root. The schema
is **nested** (there is no `default_channel` or `artifact_dir` — those are stale):

```toml
server = "https://updates.example.com"   # optional; falls back to login credentials

[app]
slug = "swarmdrop"

[app.tauri]
conf = "src-tauri/tauri.conf.json"       # release version auto-read from here
artifacts = [
  "src-tauri/target/release/bundle/msi/SwarmDrop_0.5.0_x64_en-US.msi",
  "src-tauri/target/release/bundle/msi/SwarmDrop_0.5.0_x64_en-US.msi.zip",
  "latest.json",
]

[app.android]
apk = "app/build/outputs/apk/release/app-release.apk"
```

Channel is **not** in the file — it's chosen per publish via `publish --channel <name>`. Generate or
repair this file with `swarmhive init` (see below) rather than hand-writing it when you can.

> **Cargo workspace Tauri apps**: if `src-tauri` is a *member* of a workspace whose root `Cargo.toml`
> lives above it (e.g. SwarmDrop's `crates/` + `src-tauri/`), the bundle output lands at the
> **workspace-root `target/`**, not `src-tauri/target/`. Point `artifacts` (and any `--artifact` /
> `--target <triple>` bundle path in CI) at `target/<triple>/release/bundle/...`. The example above
> assumes a standalone (non-workspace) app.

## Core workflows

### Initialize a project (`swarmhive init`)

`init` is dual-mode. When you're driving it as an agent, use the **flag-driven** form so it never
prompts:

```bash
swarmhive init --app swarmdrop --platform tauri --yes --output json
# multi-platform: repeat --platform
swarmhive init --app swarmnote --platform tauri --platform android --yes --output json
# also scaffold CI: writes .github/workflows/release.yml + prints token/secret commands (offline; --json has no prompts)
swarmhive init --app swarmdrop --platform tauri --setup-ci-token --yes --output json
```

`--setup-ci-token` writes a copy-paste `.github/workflows/release.yml` (swarmhive-action@v2 +
upload-to-draft → finalize flow) and emits `suggested_token_command`
(`swarmhive tokens create --kind api --preset ci-publish`), `github_secret_name` (`SWARMHIVE_TOKEN`),
`suggested_secret_command`, `suggested_workflow_path`, `workflow_created`. It stays offline (does not
mint the token itself — just gives the command).

`--yes` (or a non-TTY) means no prompts: it fills fields from flags + on-disk detection (`src-tauri/`
→ tauri, `android/`/`*.gradle*` → android) and only errors if it can't resolve the app slug. It
emits the `artifacts` list as a commented example — tell the user to fill in their real artifact
paths before publishing. It won't overwrite an existing `swarmhive.toml` unless you pass `--force`.

### Publish a release (the main flow)

The reliable sequence is **verify → publish → finalize → (promote)**. As of `harden-publish-flow`,
`publish` **uploads to a draft by default** (it no longer publishes automatically): it ensures a draft
release exists, uploads each artifact via presigned PUT, then completes it (release stays `draft`).
Publishing is a separate, idempotent step (`--finalize`, or `releases finalize`).

```bash
# 1. Preview first — verify (server-side duplicate check) and/or publish --dry-run (pure local plan).
swarmhive verify tauri --dry-run --output json
swarmhive publish tauri --dry-run --output json

# 2a. Multi-target Tauri: upload each target to the SAME draft, then finalize ONCE.
swarmhive publish tauri --target aarch64-apple-darwin --artifact ...arm.app.tar.gz --output json
swarmhive publish tauri --target x86_64-pc-windows-msvc --artifact ...-setup.exe --output json
swarmhive releases finalize --app swarmdrop --version 0.4.5 --output json

# 2b. Single-target one-shot: upload + publish in one go with --finalize.
swarmhive publish tauri --finalize --channel beta --notes-file CHANGELOG.md --output json

# 3. React Native Android (single target): one-shot upload + finalize.
swarmhive publish android --version 0.3.0 --version-code 30 \
  --apk app/build/outputs/apk/release/app-release.apk \
  --finalize --channel beta --notes-file CHANGELOG.md --output json
```

Key publish flags: `--finalize` publishes after uploading (omit → leave as draft); `--channel <name>`
points that channel at the release and **implies `--finalize`** (a draft can't be promoted);
`--notes-file <path>` / `--notes <text>` inject release notes (file wins, PATCHed only when changed and
**after** upload so a missing `release:update` doesn't block artifacts; `--skip-notes-update` forces
skip); `--app` overrides the config slug. The old `--no-publish` is removed (draft is now the default).
On success the JSON carries the release status, artifacts, and the update-check / download `endpoints`.

**Release-train caution**: pointing `--channel stable` at a brand-new upload ships it to all stable
users immediately. Prefer the train: publish to a pre-release channel (e.g. `--channel beta`) or with
no channel first, verify it in the wild, then move stable with `channels promote` (below). Reserve
`--channel stable` on `publish` for cases where the user explicitly wants to ship straight to stable.

**Tauri `.sig`**: if a `<artifact>.sig` (minisign) sits next to an artifact, publish uploads it
automatically for the updater to verify — no flag needed.

### Promote / roll back a channel

Releases are immutable; channels are moving pointers. Promotion and rollback only move the pointer —
they never delete history.

```bash
swarmhive channels promote  --app swarmdrop --name stable --version 0.5.0 --output json
swarmhive channels rollback --app swarmdrop --name stable --output json            # → previous release
swarmhive channels rollback --app swarmdrop --name stable --to-version 0.4.5 --output json
```

### Adjust rollout / force-update policy

Release policy lives on the release row and is changed with `releases update` (no artifact upload):

```bash
swarmhive releases update --app swarmdrop --version 0.5.0 --rollout-percent 20 --output json
swarmhive releases update --app swarmdrop --version 0.5.0 --min-version 0.4.0 --output json
swarmhive releases update --app swarmnote-rn --version 0.5.0 --android-min-version-code 40 --output json
```

Sentinel clears: `--rollout-percent 100` disables gray rollout, and `--min-version 0.0.0` removes
the Tauri force-update floor. Omitting a flag means "leave unchanged".

### Inspect state

```bash
swarmhive apps list --output json
swarmhive releases list --app swarmdrop --output json
swarmhive artifacts list --app swarmdrop --version 0.5.0 --output json
swarmhive channels list --app swarmdrop --output json
swarmhive telemetry overview --days 7 --output json
```

## Safety — these actions reach the outside world

Publishing, promoting, rolling back, deleting, and yanking change what real users download. Treat
them like outward-facing actions:

- Before a **real publish / promote / rollback**, confirm intent with the user (or that you're in an
  authorized CI run). When unsure, run `verify` / `publish --dry-run` first and show the plan.
- For **destructive** verbs (`apps delete`, `releases yank`, `mail providers delete`), restate what
  will be destroyed and only proceed with `--yes` after the user agrees.
- Roll back by **moving the channel**, never by deleting a release.
- Never paste a real secret into a plaintext flag; route it through `--secret-stdin` or env.

## Error handling

On failure the process exits with a tiered code (`2` permanent / `1` retryable) and (with
`--output json`) writes RFC 9457 problem+json to stderr. Read the `detail` (or `title`) field for the
cause and relay it. Common cases: missing `SWARMHIVE_TOKEN` (auth), `409`/conflict (version already
exists — fine for re-publish), forbidden (the token lacks a permission — `required_permission` +
`remediation_hint` are in the problem body), `upload_checksum_mismatch` (artifact changed mid-upload).

**`403` on re-publish is usually a missing `release:update`** (the token can create/publish but not
PATCH notes). Fix by recreating the CI token with the full publish set:
`swarmhive tokens create --kind api --preset ci-publish` (the `remediation_hint` says exactly this).
The CLI already conditional-skips the notes PATCH when notes are unchanged, so a missing
`release:update` no longer blocks the artifact upload — but the token should still carry it.

## Full command surface

`references/command-reference.md` has every subcommand, flag, env var, and the storage/mail admin
surface. Read it when the task goes beyond the workflows above (storage backend setup, mail provider
config, app/release CRUD, less-common flags).
