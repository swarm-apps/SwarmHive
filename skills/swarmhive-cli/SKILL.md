---
name: swarmhive-cli
description: >-
  Drive the SwarmHive `swarmhive` CLI to publish and manage app update releases. Use this skill
  WHENEVER the user wants to publish / ship / release a Tauri desktop or React Native Android app
  update to a SwarmHive server, scaffold `swarmhive.toml`, run `swarmhive init / verify / publish`,
  promote or roll back a release channel (e.g. beta → stable), manage SwarmHive storage backends or
  mail providers, or wire SwarmHive publishing into CI/CD — even if they only say "swarmhive",
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

- Ship built artifacts (installer / APK) to the server → `publish tauri` / `publish android`.
- Just move a channel pointer (beta → stable, or undo) → `channels promote` / `channels rollback`.
- Set up a repo to publish → `swarmhive init` (writes `swarmhive.toml`).
- Look before you leap → `verify tauri|android` (artifact check + server duplicate check) or `publish … --dry-run` (pure local plan).
- Inspect state → `apps list`, `releases list`, `channels list`, `artifacts list`.
- Server admin (storage backend, SMTP) → `storage …` / `mail …` (see the reference).

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
  one JSON object/array on stdout**; **failure → an RFC 9457 problem+json object on stderr**; and
  **the exit code is non-zero on any failure**. Parse stdout for results, read stderr's `detail` /
  `title` for the error reason. Default (no flag) is human-readable tables.
- **Destructive ops require `--yes`.** `apps delete`, `releases yank`, `mail providers delete` refuse
  to run without `--yes` — there is no interactive confirmation. This is your safety interlock (see
  Safety below).
- **Secrets never go in plaintext flags.** For S3 / SMTP secrets prefer `--secret-stdin` (pipe the
  value) or the env vars `SWARMHIVE_STORAGE_SECRET` / `SWARMHIVE_MAIL_PASSWORD`. A plaintext
  `--access-key-secret` / `--password` flag leaks into shell history and `ps` — only use it if the
  user explicitly accepts that.
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

## Core workflows

### Initialize a project (`swarmhive init`)

`init` is dual-mode. When you're driving it as an agent, use the **flag-driven** form so it never
prompts:

```bash
swarmhive init --app swarmdrop --platform tauri --yes --output json
# multi-platform: repeat --platform
swarmhive init --app swarmnote --platform tauri --platform android --yes --output json
```

`--yes` (or a non-TTY) means no prompts: it fills fields from flags + on-disk detection (`src-tauri/`
→ tauri, `android/`/`*.gradle*` → android) and only errors if it can't resolve the app slug. It
emits the `artifacts` list as a commented example — tell the user to fill in their real artifact
paths before publishing. It won't overwrite an existing `swarmhive.toml` unless you pass `--force`.

### Publish a release (the main flow)

The reliable sequence is **verify → publish → (promote)**. `publish` ensures a draft release exists,
uploads each artifact via presigned PUT, then completes (and by default publishes) it.

```bash
# 1. Preview first — verify (server-side duplicate check) and/or publish --dry-run (pure local plan).
swarmhive verify tauri --dry-run --output json
swarmhive publish tauri --dry-run --output json

# 2. Tauri: version auto-read from tauri.conf.json; --target sets the triple; --notes-file injects changelog.
swarmhive publish tauri --channel beta --notes-file CHANGELOG.md --output json

# 3. React Native Android: version + versionCode are explicit flags; --apk overrides config.
swarmhive publish android --version 0.3.0 --version-code 30 \
  --apk app/build/outputs/apk/release/app-release.apk \
  --channel beta --notes-file CHANGELOG.md --output json
```

Key publish flags: `--channel <name>` promotes that channel to the release after publishing;
`--notes-file <path>` / `--notes <text>` inject release notes (file wins); `--no-publish` uploads but
leaves the release in draft; `--app` overrides the config slug. On success the JSON carries the
release status, artifacts, and the update-check / download `endpoints` — surface those to the user.

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

### Inspect state

```bash
swarmhive apps list --output json
swarmhive releases list --app swarmdrop --output json
swarmhive artifacts list --app swarmdrop --version 0.5.0 --output json
swarmhive channels list --app swarmdrop --output json
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

On failure the process exits non-zero and (with `--output json`) writes RFC 9457 problem+json to
stderr. Read the `detail` (or `title`) field for the cause and relay it. Common cases: missing
`SWARMHIVE_TOKEN` (auth), `409`/conflict (version already exists — that's fine for re-publish),
forbidden (the token lacks a permission — its `required_permission` is in the problem body),
`upload_checksum_mismatch` (artifact changed mid-upload).

## Full command surface

`references/command-reference.md` has every subcommand, flag, env var, and the storage/mail admin
surface. Read it when the task goes beyond the workflows above (storage backend setup, mail provider
config, app/release CRUD, less-common flags).
