# SwarmHive CLI

[![crates.io](https://img.shields.io/crates/v/swarmhive-cli.svg)](https://crates.io/crates/swarmhive-cli)
[![npm](https://img.shields.io/npm/v/@swarm-hive/cli.svg)](https://www.npmjs.com/package/@swarm-hive/cli)

Command-line release entrypoint for [SwarmHive](https://github.com/swarm-apps/swarmhive) — a self-hosted
update distribution hub for **Tauri** desktop apps and **React Native Android** apps.

The CLI is the **first-class publish path** (local and CI/CD): it authenticates against your SwarmHive
server, uploads artifacts straight to your S3-compatible storage via presigned URLs, and manages apps,
releases, channels, and storage. The binary is `swarmhive` (distributed as `swarmhive-cli` on crates.io
and `@swarm-hive/cli` on npm).

## Install

Pick whichever fits — every method installs the same `swarmhive` binary.

**Homebrew** (macOS / Linux):

```sh
brew install swarm-apps/tap/swarmhive
```

**Shell script** (macOS / Linux):

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/swarm-apps/swarmhive/releases/latest/download/swarmhive-cli-installer.sh | sh
```

**PowerShell** (Windows):

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/swarm-apps/swarmhive/releases/latest/download/swarmhive-cli-installer.ps1 | iex"
```

**npm** (cross-platform, no Rust toolchain — handy in CI):

```sh
npm i -g @swarm-hive/cli      # or run on demand: npx @swarm-hive/cli@latest <command>
```

**Cargo** (build from source via crates.io):

```sh
cargo install swarmhive-cli
```

**Prebuilt binaries** for Linux / macOS / Windows (x64 + arm64) are attached to every
[GitHub Release](https://github.com/swarm-apps/swarmhive/releases).

## Authenticate

```bash
swarmhive login https://updates.example.com   # browser device flow → stores a PAT (0600)
swarmhive logout                              # revoke the PAT and remove the local file
```

`login` uses the RFC 8628 device flow: it prints a code, opens your browser, you approve (password or
GitHub), and the CLI writes a Personal Access Token to `~/.config/swarmhive/credentials.toml`. The
`SWARMHIVE_TOKEN` env var overrides the file when both are present (use a scoped token in CI).

## Publish a release

```bash
# Tauri desktop
swarmhive publish tauri --app my-app --channel stable --artifact ./dist/My_App_1.2.0_x64-setup.exe

# React Native Android
swarmhive publish android --app my-app --version 1.2.0 --version-code 42 --abi arm64-v8a \
  --channel stable --artifact ./app/build/outputs/apk/release/app-release.apk
```

Artifacts are uploaded directly to your storage (presigned PUT), then the server records the release and
promotes the given channel. Use `swarmhive verify tauri|android ...` for a dry run that validates without
uploading. In GitHub Actions, the official [`swarm-apps/swarmhive` publish action](../../.github/actions/publish)
wraps `npx @swarm-hive/cli publish`.

## Command surface

| Command | What it does |
| --- | --- |
| `login [server]` / `logout` | Device-flow auth; store / revoke a local PAT. |
| `verify tauri\|android` | Validate a release without uploading (dry run). |
| `publish tauri\|android` | Upload artifacts and create + publish a release. |
| `apps list\|get\|create\|update\|delete` | Manage apps. |
| `releases list\|get\|create\|update\|publish\|yank` | Manage releases (drafts, publish, yank). |
| `channels list\|create\|set-default\|promote\|rollback` | Manage release channels (the "release train" pointers). |
| `artifacts list` | Inspect a release's artifacts. |
| `storage …` | Configure storage backends (e.g. `storage init rustfs`). |
| `mail …` | Manage mail providers / templates / logs. |
| `version` | Print version information. |

> `init` (scaffold a `swarmhive.toml`) is reserved but not yet implemented.

Global flags: `--output table|json` (machine-readable output for scripts/CI), `--help` on any subcommand.

## Configuration

- `SWARMHIVE_TOKEN` — scoped API token; takes precedence over the credentials file. Use this in CI.
- `SWARMHIVE_SERVER` — default server URL when a command doesn't take one explicitly.
- `~/.config/swarmhive/credentials.toml` — written by `login` (mode 0600).

## License

Apache-2.0 © swarm-apps
