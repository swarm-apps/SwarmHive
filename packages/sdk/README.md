# @swarm-hive/sdk

[![npm](https://img.shields.io/npm/v/@swarm-hive/sdk.svg)](https://www.npmjs.com/package/@swarm-hive/sdk)
[![license](https://img.shields.io/npm/l/@swarm-hive/sdk.svg)](https://github.com/swarm-apps/swarmhive/blob/main/LICENSE)

Headless client update SDK for [SwarmHive](https://github.com/swarm-apps/swarmhive) — a self-hosted update
distribution hub for **Tauri** desktop apps and **React Native Android** apps.

This package is the **zero-platform-dependency core**: an 8-state update engine plus a `ports & adapters`
contract. It contains **no** `@tauri-apps/*`, `expo-*`, or `react-native` imports — platform adapters and
the UI components are distributed separately through shadcn registries (see [Ecosystem](#ecosystem)).

## Install

```bash
npm i @swarm-hive/sdk
# or: pnpm add @swarm-hive/sdk / yarn add @swarm-hive/sdk
```

ESM-only. `react` is an **optional** peer dependency — required only if you import the `@swarm-hive/sdk/react`
sub-entry.

## How it fits together

```text
@swarm-hive/sdk            ← this package (headless: engine + ports, zero platform deps)
      ▲  UpdateAdapter (the only contract)
      │
  platform adapter + hook + UI   ← shadcn registries (you copy the source into your app)
   ├─ @swarmhive      → Tauri / Electron / Web   (packages/registry-web)
   └─ @swarmhive-rn   → React Native / Expo       (packages/registry-rn)
```

The SDK never talks to a platform directly. It drives everything through one interface — `UpdateAdapter`
(`check` / `download` / `install` / `storage` / `compare`). The Tauri and React Native adapters implement
that interface and live in the registries, because platform glue is exactly the part you want to own and
edit in your own source tree.

## Quick start

```ts
import { createUpdateEngine, type UpdateAdapter } from "@swarm-hive/sdk";

// In a real app you get `adapter` from a shadcn registry (tauriAdapter / rnAdapter).
// You only build one by hand for tests or custom platforms.
const adapter: UpdateAdapter = /* ... */;

const engine = createUpdateEngine(adapter, {
  currentVersion: "1.2.0", // Tauri: semver · RN: versionCode as string
  clientId: "stable-uuid", // from ensureClientId(adapter.storage)
});

const state = engine.getState();
await state.check();        // idle → checking → up-to-date | available | force-required | error
await state.download();     // available/force-required → downloading → ready
await state.install();      // ready → (relaunch / system installer)
```

### React

```tsx
import { useUpdateEngine } from "@swarm-hive/sdk/react";

function UpdateBanner({ engine }) {
  const { status, release, progress, check, download, install } = useUpdateEngine(engine);
  if (status === "available") return <button onClick={download}>Update to {release.version}</button>;
  if (status === "downloading") return <progress value={progress?.percent} />;
  if (status === "ready") return <button onClick={install}>Restart & install</button>;
  return null;
}
```

`useUpdateEngine(engine, selector?)` subscribes to the engine's zustand store; pass a selector to re-render
only when the slice you care about changes.

## Core concepts

### The 8-state engine

`createUpdateEngine(adapter, opts)` returns a zustand vanilla store (`UpdateEngine`). Its `status` moves
through eight states, kept in sync with SwarmHive's server and the real apps that use it:

```text
idle → checking → up-to-date
                → available        → downloading → ready → (install)
                → force-required   → downloading → ready → (install)
                → error  (retryable)
```

`UpdateEngineState` exposes `status`, `release`, `progress`, `error` and the actions:

| action | when | what it does |
| --- | --- | --- |
| `check(force?)` | any time | hits the endpoint via the adapter. `force` bypasses the recheck throttle (manual click / app-resume). |
| `download()` | `available` / `force-required` | downloads with progress, moves to `ready`. |
| `install()` | `ready` | installs (Tauri relaunch / Android system installer). |
| `postpone(ttlMs?)` | `available` | "remind me later" — persists a dismiss-TTL; same version stays quiet (force updates ignore it). |
| `retry()` | `error` | forces a fresh check. |
| `acknowledgeError()` | `error` | clears the error, returns to the prior visible state. |

`EngineOptions`: `currentVersion`, `clientId`, optional `dismissTtlMs` (default 24h) and `recheckIntervalMs`
(default 12h).

### The `UpdateAdapter` port

```ts
interface UpdateAdapter {
  check(ctx: CheckContext): Promise<ReleaseInfo | null>;
  download(release: ReleaseInfo, onProgress: (p: Progress) => void): Promise<DownloadHandle>;
  install(handle: DownloadHandle): Promise<void>;
  storage: KeyValueStorage;
  compare(current: string, candidate: ReleaseInfo): boolean;
}
```

This is the spine of the SDK. Implement it once per platform (or grab the ready-made `tauriAdapter` /
`rnAdapter` from the registries).

## Endpoint helpers

If you write your own adapter, these normalize SwarmHive's HTTP responses into `ReleaseInfo`:

- **`checkUpdate(opts)`** — `GET /api/v1/updates/tauri/:appSlug` (Tauri). `204 → null`, `200 → ReleaseInfo`,
  `4xx/5xx → throw UpdateError`. Takes `baseUrl`, `appSlug`, `currentVersion`, `target`, `arch`, optional
  `channel` / `clientId` / `fetchImpl`.
- **`checkUpdateAndroid(opts)`** — `GET /api/v1/updates/android/:appSlug` (RN Android). Flat `has_update`
  response (`has_update:false → null`). Takes `baseUrl`, `appSlug`, `currentVersionCode`,
  `currentVersionName`, optional `abi` / `channel` / `clientId` / `runtimeVersion` / `fetchImpl`.
  `normalizeAndroid(body, channel)` is exported for custom transports.

## Utilities

- **`ensureClientId(storage, generateId?)`** — reads or creates a stable `client_id` (persisted via the
  adapter's `storage`). The `client_id` is the key SwarmHive uses for gradual-rollout bucketing. On Hermes
  (no global `crypto.randomUUID`) pass `generateId` (e.g. `expo-crypto`'s `randomUUID`).
- **`inRolloutBucket(clientId, percent)`** — deterministic rollout gate (blake3, bit-aligned with the Rust
  server) so a client consistently lands in or out of a staged rollout.
- **`semverComparator(current, candidate)` / `versionCodeComparator(current, candidate)`** — the two
  `compare` strategies: semver for Tauri, integer versionCode for Android.

## Types

`ReleaseInfo`, `Progress`, `UpgradeType` (`"prompt" | "force" | "silent"`), `UpdateStatus`, `UpdateError`
(carries a `phase` of `"check" | "download" | "install"`), plus the ports types `CheckContext`,
`DownloadHandle`, `KeyValueStorage`. `ReleaseInfo.kind` (`"native-package" | "ota-bundle"`) is the seam for
future OTA — MVP only ever produces `native-package`.

## Ecosystem

The adapters and copy-paste UI components are **not** in this package (that keeps the core zero-dependency
and lets you own the platform glue). Add them with shadcn:

```bash
# Tauri / desktop
npx shadcn@latest add @swarmhive/update-provider
# React Native / Expo
npx shadcn@latest add @swarmhive-rn/update-provider
```

## License

Apache-2.0 © swarm-apps
