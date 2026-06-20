## ADDED Requirements

### Requirement: registry-rn SHALL be a standalone shadcn registry source package

`packages/registry-rn` SHALL be its own shadcn registry package — a dedicated `registry.json` catalog plus `registry/rn/<item>/` sources, its own namespace, its own `public/r/` build output, and its own `build:registry` script (`shadcn build`) — and SHALL NOT be added under `registry-web`. Mixing RN items into `registry-web` MUST be avoided because it would break `registry-web`'s hardcoded 9-item build assertion and pollute the `@swarmhive` web namespace (consumers would resolve web components). The package's own `tsconfig.json` `@/*` paths SHALL point to `./registry/rn/*`.

#### Scenario: RN registry lives in its own package

- **WHEN** the repository is inspected for the RN update UI registry
- **THEN** `packages/registry-rn` exists with its own `registry.json`, `registry/rn/<item>/` sources, namespace, and `public/r/` output
- **AND** no RN item is added to `packages/registry-web`, leaving its 9-item build assertion and `@swarmhive` web namespace untouched

#### Scenario: build produces flat registry JSON

- **WHEN** `pnpm --filter @swarm-hive/registry-rn build:registry` runs
- **THEN** `public/r/registry.json` and a JSON per item are produced under `packages/registry-rn`
- **AND** each item's `files[].content` is inlined from source

### Requirement: rnAdapter SHALL be an injectable factory implementing UpdateAdapter

The `rnAdapter` SHALL be created via an injectable factory `createRnAdapter({ downloader, installer, storage })` and SHALL return an `UpdateAdapter` object literal `{ storage, compare, check, download, install }`, mirroring `tauriAdapter`'s shape. The `rn-adapter.ts` source SHALL import only `@swarm-hive/sdk` and a sibling `ports.ts` (the injectable `ApkDownloader` / `ApkInstaller` interfaces) and SHALL NOT import any `expo-*` module, delegating all platform I/O to the injected `downloader` / `installer` so the adapter logic SHALL be unit-testable in node with fakes. `compare` SHALL be the SDK's `versionCodeComparator`. The SDK `UpdateAdapter` interface and `install` port signature SHALL remain unchanged.

#### Scenario: adapter logic is pure and injectable

- **GIVEN** `createRnAdapter` called with fake `downloader` / `installer` / `storage`
- **WHEN** the `rn-adapter.ts` module is loaded in a node test
- **THEN** it imports only `@swarm-hive/sdk` and its sibling `ports.ts`, resolving without requiring any `expo-*` module
- **AND** `compare` is the SDK `versionCodeComparator` and the returned object exposes `{ storage, compare, check, download, install }`

### Requirement: rnAdapter check SHALL delegate to checkUpdateAndroid without re-normalizing

`rnAdapter.check(ctx)` SHALL delegate to the SDK's `checkUpdateAndroid` (which already returns a normalized `ReleaseInfo | null`), mapping `ctx.currentVersion`/`ctx.clientId` into `CheckUpdateAndroidOptions` with `currentVersionCode = Number(ctx.currentVersion)` and `clientId = ctx.clientId`, and SHALL return that normalized result directly with no adapter-side normalize pass (thinner than `tauriAdapter`).

#### Scenario: check delegates to checkUpdateAndroid

- **GIVEN** an `rnAdapter` created via `createRnAdapter`
- **WHEN** `rnAdapter.check(ctx)` runs
- **THEN** it calls the SDK `checkUpdateAndroid` with `currentVersionCode = Number(ctx.currentVersion)` and `clientId = ctx.clientId`
- **AND** it returns that normalized `ReleaseInfo | null` directly, with no adapter-side normalize step

### Requirement: rnAdapter download SHALL produce a serializable APK-path payload via the injected downloader

`rnAdapter.download` SHALL delegate to the injected `downloader`, which SHALL use `expo-file-system/legacy` `createDownloadResumable` to fetch the APK to `cacheDirectory`, feeding cumulative `totalBytesWritten`/`totalBytesExpectedToWrite` into a `DownloadSpeedTracker` (a platform UI concern living in the adapter, NOT in sdk-core) to produce the SDK `Progress { downloaded, total, percent, speed? }`, and SHALL clear any leftover partial file before downloading. The resulting `DownloadHandle.payload` SHALL be the APK local-path string (serializable), so the adapter SHALL NOT cache the handle in a closure — `install` reads `handle.payload` directly.

#### Scenario: download maps progress and exposes APK path as payload

- **WHEN** `rnAdapter.download` runs and the downloader reports cumulative progress
- **THEN** `onProgress` receives a `Progress` with a computed `percent`, an optional `speed`, and a final `percent` of 1 at completion
- **AND** the resolved `DownloadHandle.payload` is the APK local-path string, requiring no closure-cached handle for the later `install`

### Requirement: installer SHALL install via expo-intent-launcher ACTION_VIEW with no native code

The injected `installer` SHALL install the APK with zero native code by calling `IntentLauncher.startActivityAsync("android.intent.action.VIEW", { data, type: "application/vnd.android.package-archive", flags: FLAG_GRANT_READ_URI_PERMISSION | FLAG_ACTIVITY_NEW_TASK })`, where `data` is a `content://` URI obtained from `FileSystem.getContentUriAsync(path)` using `expo-file-system`'s built-in FileProvider. It SHALL NOT ship a custom FileProvider, `res/xml/file_paths.xml`, a custom authority, or any Kotlin/Java native module. Because the built-in FileProvider supplies the content:// URI and grants read permission via the flag, the API 24+ `FileUriExposedException` is avoided and no `${applicationId}.fileprovider` authority conflict with rn-fetch-blob/blob-util can arise. This mirrors the SwarmDrop-RN / SwarmNote-RN production installer.

#### Scenario: APK is handed to the system installer via ACTION_VIEW

- **WHEN** the installer installs an APK at a cached local path
- **THEN** it converts the path to a `content://` URI via `getContentUriAsync` and dispatches an `ACTION_VIEW` intent typed `application/vnd.android.package-archive` with `FLAG_GRANT_READ_URI_PERMISSION | FLAG_ACTIVITY_NEW_TASK`
- **AND** it ships no custom FileProvider, no `file_paths.xml`, and no Kotlin/Java native module

### Requirement: install SHALL be a fire-and-forget handoff that resolves on intent dispatch and SHALL NOT relaunch

`rnAdapter.install(handle)` SHALL call the injected installer with `handle.payload` (the APK path) and SHALL NOT relaunch the app — relaunch belongs to Tauri only; on RN the system installer / user restart drives the restart, and blindly copying `tauriAdapter`'s `relaunch()` is a bug. The installer SHALL return `Promise<void>` and SHALL resolve once the install intent has been dispatched (handoff), NOT await any install result, because once the system confirmation UI takes over the committing process is replaced on success and there is no reliable callback on cancel. The truth of a successful install SHALL be a next-cold-start `versionCode` recheck, not this Promise. This SHALL require zero changes to the SDK's 8-state engine (which has no `installing` state and treats install as fire-and-forget) or the `install` port signature.

#### Scenario: install hands off and resolves at intent dispatch without relaunch

- **WHEN** `rnAdapter.install(handle)` runs after a successful download
- **THEN** it invokes the installer with the APK path from `handle.payload`, which resolves `void` once the `ACTION_VIEW` intent is dispatched rather than awaiting an install result
- **AND** it does NOT call any relaunch API, and the SDK engine, its 8 states, and the `install` port signature are unchanged

#### Scenario: self-update success is verified out-of-band

- **GIVEN** the user confirms the system install dialog and the process is replaced by the new APK
- **WHEN** the app next cold-starts
- **THEN** the actual install success is confirmed by a `versionCode` recheck (`versionCodeComparator` / `checkUpdateAndroid`), not by the resolved install Promise
- **AND** the engine being left at `ready` after a handoff is harmless (no `installing` state, no timeout)

### Requirement: config plugin SHALL inject only REQUEST_INSTALL_PACKAGES

The config plugin SHALL inject only the single `android.permission.REQUEST_INSTALL_PACKAGES` uses-permission, via `withAndroidManifest` from `@expo/config-plugins`, de-duplicating before adding (mirroring the SwarmDrop-RN `with-android-install-permission.js` production plugin). It SHALL NOT inject a FileProvider, `file_paths.xml`, any authority, or any other permission, so the rn-fetch-blob/blob-util `${applicationId}.fileprovider` authority conflict cannot arise.

#### Scenario: prebuild injects one permission and no FileProvider

- **WHEN** the config plugin runs during prebuild
- **THEN** `REQUEST_INSTALL_PACKAGES` is the only uses-permission it ensures in the manifest, added once even if already present
- **AND** it injects no FileProvider, no `file_paths.xml`, no authority, and no other permission

### Requirement: native forced update SHALL be soft-enforced with AppState-driven re-prompting

The native "forced update" SHALL be treated as soft enforcement: the system install confirmation dialog's cancel/back affordances are rendered by `system_server` and the app SHALL NOT assume it can suppress them. Because dismissing the system dialog (cancel / back / tap-outside) yields no reliable callback (leaving the engine at `ready` with no `installing` state or timeout), continued re-prompting SHALL NOT depend on a system callback: the `UpdateProvider` SHALL, on `AppState` returning to `active`, actively `check()` and let the engine recheck the installed `versionCode`, returning to `force-required`/`available` when still not installed.

#### Scenario: no-callback dismissal is recovered by AppState recheck

- **GIVEN** a forced update whose system install dialog the user dismisses via cancel / back / tap-outside, producing no reliable callback
- **WHEN** the app returns to the foreground (`AppState` → `active`)
- **THEN** the `UpdateProvider` actively runs `check()` and the engine rechecks the installed `versionCode` rather than waiting on a system callback
- **AND** when still not installed, the engine returns to `force-required` (or `available`) to keep prompting

### Requirement: RN UI components SHALL use NativeWind + React Native Reusables and mirror the registry-web set

The registry SHALL ship the same RN UI set as registry-web (`useUpdate`/`UpdateProvider`/release-notes/prompt/force/progress/settings), built on **NativeWind className + React Native Reusables primitives** (`Dialog`/`AlertDialog`/`Button`/`Progress`/`Text`), using **semantic tokens only** (`bg-background`/`bg-muted`/`text-foreground`/`text-muted-foreground`/`text-primary`/`text-destructive`/`border-border`) — colors come from the consumer's own `global.css` (auto-adapting to each app's theme, dark mode for free); the registry SHALL NOT hardcode colors. UI primitives SHALL be depended on via the **`@react-native-reusables/*` namespace** in `registryDependencies` (consumer registers it in `components.json`), and the registry SHALL NOT list bare `dialog`/`button`/`progress`/`text`/`alert-dialog` (those resolve to web Radix/@shadcn). RNR primitives SHALL NOT be vendored as `registry.json` items (consumers pull canonical from the RNR registry); vendored copies under `registry/rn/components/ui/` exist for local typecheck only. Dialog mapping: prompt → `Dialog` (dismissable, close X + controlled `open`/`onOpenChange`); force / progress → `AlertDialog` (non-dismissable, no X). Dialog/AlertDialog overlays SHALL set layout + `rgba` background via inline `style` (NativeWind v5 preview's `react-native-css` drops alpha colors), and the components SHALL document the RNR `PortalHost` prerequisite. RN-specific substitutions SHALL apply: `expo-application` for the `versionCode`, `ensureClientId` passed `generateId = () => Crypto.randomUUID()` (expo-crypto, required so Hermes does not throw and rollout bucketing keeps working), and `AppState 'change' → 'active'` in place of `window` focus. The `update-texts` module SHALL be copied into `registry/rn/lib/` with added RN-only keys (not forked from web), and the `auto-install-on-ready` (`useEffect` install when `status === "ready"`) pattern SHALL be carried over. This SHALL require zero changes to the server or SDK.

> Amended 2026-06-05 — the original "primitive-only `View`/`Text`/`Modal`/`StyleSheet` with hardcoded hex" approach was replaced by NativeWind + React Native Reusables (RNR / `@rn-primitives`) so the RN UI matches registry-web's shadcn semantic-token model. See [dev-notes/knowledge/architecture.md](../../../../dev-notes/knowledge/architecture.md) "registry-rn 样式" and the production reference `SwarmNote-RN/src/components/update/*`.

#### Scenario: components are NativeWind + RNR and reuse the SDK engine

- **WHEN** the registry-rn component sources are inspected
- **THEN** they use NativeWind className with semantic tokens (no hardcoded hex / `StyleSheet`), compose RNR `Dialog`/`AlertDialog`/`Button`/`Progress`/`Text`, drive state through `useUpdate` (built on the SDK engine, comparator, and rollout — not reimplemented), and list no bare `dialog`/`button`/`progress` web `registryDependencies`
- **AND** they substitute `expo-application` versionCode, `Crypto.randomUUID` as `ensureClientId`'s `generateId`, and `AppState` for window focus, with a copied `update-texts` carrying RN-only keys

#### Scenario: chained component install pulls the hook, adapter, and RNR primitives

- **GIVEN** the user has mapped both the `@swarmhive-rn` and `@react-native-reusables` namespaces in `components.json`
- **WHEN** the user runs `shadcn add` for a single RN component
- **THEN** its `useUpdate` hook and `rnAdapter` are transitively installed via the `@swarmhive-rn/*` `registryDependencies`
- **AND** the RNR primitives it composes are transitively installed from the `@react-native-reusables/*` registry, with no web Radix pulled in
