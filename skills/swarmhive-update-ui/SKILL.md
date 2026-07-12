---
name: swarmhive-update-ui
description: >-
  Integrate the SwarmHive in-app update UI into a host app — the client-side
  counterpart to publishing (which the `swarmhive-cli` skill covers). Use this
  skill WHENEVER the user wants to add / wire up / show an update prompt,
  "check for updates", a force-update gate, a download-progress dialog, or an
  "update available" banner in a Tauri desktop / Electron / web app or an Expo
  React Native Android app that ships through a SwarmHive server — even if they
  only say "add the update dialog", "wire in auto-update", "use the SwarmHive
  components", "shadcn add the update UI", "@swarmhive / @swarmhive-rn registry",
  `UpdateProvider`, `useUpdate`, `createTauriAdapter` / `createRnAdapter`, or name
  a swarm-apps product (SwarmDrop, SwarmNote, SwarmNote-RN). It distributes UI via
  shadcn registries (registry-web for Tauri/web, registry-rn for Expo) over a
  headless `@swarm-hive/sdk` engine, and encodes the non-obvious wiring + the
  pitfalls that break a naive integration (transport scheme, PortalHost,
  versionCode, channel defaults, i18n). Prefer this over hand-writing update UI
  or guessing the registry namespace / adapter API.
---

# Integrating the SwarmHive update UI

SwarmHive ships **no UI as an npm package**. The client update experience is a
headless engine (`@swarm-hive/sdk`, the 8-state machine + ports) plus **UI source
code distributed through shadcn registries** that you copy into your app and own.
Two registries, picked by platform:

| Host app | Registry namespace | Adapter | UI primitives it needs |
| --- | --- | --- | --- |
| Tauri desktop / Electron / web | `@swarmhive` (registry-web) | `createTauriAdapter` (wraps `@tauri-apps/plugin-updater`) | shadcn (Radix) — dialog/button/progress |
| Expo React Native **Android** | `@swarmhive-rn` (registry-rn) | `createRnAdapter` (SDK `checkUpdateAndroid` + expo installer) | NativeWind + React Native Reusables (RNR) |

**Why a registry, not a component lib**: platform adapters need the user to edit
source (endpoint, install behaviour), and a zero-dependency npm core is the most
stable. So the *adapter + hook + dialogs* land in your repo as editable files.

**GitHub Release mirror sources** (server ≥ 0.7.0, `add-github-release-source`): when the server
records a GitHub Release mirror for an artifact, the RN adapter's `download()` automatically falls
back to the mirror (`ReleaseInfo.mirrorUrls`, from the Android update response's `mirror_urls`) if the
primary source fails, and the web `download-panel` renders each artifact's `sources[]` (S3 + GitHub)
as multiple download options. Both are built into the shipped registry source — no integrator wiring.

This skill is the **client half**. The server/publish half (`swarmhive publish`,
channels, `swarmhive.toml`) is the **`swarmhive-cli`** skill — reach for that when
the task is shipping a release, not wiring the UI.

## Read the platform reference before writing code

The 5-step flow below is platform-agnostic; the exact `shadcn add` items,
`UpdateProvider` props, and config edits differ. **Read the matching reference
in full before editing** — the differences are where integrations break:

- Tauri / web → [references/web-tauri.md](references/web-tauri.md)
- Expo React Native Android → [references/rn-expo.md](references/rn-expo.md)
- The cross-cutting pitfalls (every integration hits some) → [references/gotchas.md](references/gotchas.md)

## The integration flow (both platforms)

### 1. Confirm the host-app prerequisites

The registry copies *source* into an app that already has the UI toolkit:

- **Web/Tauri**: `components.json` exists (shadcn initialized), Radix-based
  `dialog`/`button`/`progress` resolvable from `@shadcn`, and the Tauri updater
  plugin installed + a minisign `pubkey` in `tauri.conf.json`.
- **Expo/RN**: NativeWind + RNR set up, `components.json` present, a `PortalHost`
  mounted at the app root (RNR dialogs render through a Portal — **without it
  they silently don't appear**), and `expo-file-system` + `expo-intent-launcher`
  available (the install chain).

If a prerequisite is missing, set it up first — the registry items declare these
as `registryDependencies` and will pull shadcn/RNR primitives, but the *host
wiring* (PortalHost, updater plugin) is yours.

### 2. Register the SwarmHive registry namespace in `components.json`

The registry is served as static JSON from GitHub raw (not a server endpoint —
`shadcn add` is a dev-time op, the project is open source). Add to `registries`:

- Web: `"@swarmhive": "https://raw.githubusercontent.com/swarm-apps/swarmhive/main/packages/registry-web/public/r/{name}.json"`
- RN: `"@swarmhive-rn": ".../packages/registry-rn/public/r/{name}.json"` **plus**
  `"@react-native-reusables": "https://reactnativereusables.com/r/nativewind/{name}.json"` (the RN dialogs pull RNR primitives from there).

Pin a tag instead of `main` for reproducibility once you've verified an integration.

### 3. `shadcn add` the update components

Add the umbrella items; their `registryDependencies` cascade to the adapter, the
`useUpdate` hook, `release-notes-view`, `update-texts`, and the shadcn/RNR
primitives. Use the platform reference for the exact list. Two universal snags:

- `shadcn add --yes` does **not** auto-answer per-file *overwrite* prompts. Pipe
  `yes n |` so existing `ui/*` (dialog/button/utils) are kept, not clobbered.
- If a copied file imports `./ports` (RN) or any sibling that wasn't created,
  the registry item's `files` manifest is missing it — see gotchas; copy the
  canonical file from the registry source as a stopgap and report the registry bug.

### 4. Wire `<UpdateProvider>` + the dialogs

`UpdateProvider` assembles the engine (adapter + current version + persisted
`clientId`) and provides it via context; the dialogs subscribe with `useUpdate()`
and must live **inside** it. The provider's props are where the platforms diverge
— **Tauri reads the endpoint from `tauri.conf.json` (no props needed); RN needs
`baseUrl` + `appSlug` passed explicitly** (there's no conf file to read). Compose
a small host: open the prompt when `status` becomes `"available"`, let
`ForceUpdateDialog` self-manage on `"force-required"`, and let the progress dialog
self-manage on `downloading`/`ready`. Handle `"error"` — don't render nothing and
don't let a status badge mislabel a failed check as "up to date".

### 5. Configure the endpoint + transport, then verify

Point the client at the SwarmHive server and **make the transport scheme work**:
a plain-`http://` dogfood server is rejected by default on both platforms (Tauri
`InsecureTransportProtocol` in release builds; Android cleartext block) — the
references give the exact flag. Then verify against the server: an older
version/versionCode must get an update, the current one must not. Remember the
update endpoint serves the **stable** channel by default — a `--channel beta`
publish needs `swarmhive channels promote` before the client sees it.

## What "done" looks like

- The update components are in the app's source tree and typecheck/build clean.
- `<UpdateProvider>` wraps the tree (with PortalHost for RN); dialogs render on
  the right states; error state is handled.
- The endpoint is configured, transport works, and a real check against the
  server returns an update for an old version and none for the current one.
