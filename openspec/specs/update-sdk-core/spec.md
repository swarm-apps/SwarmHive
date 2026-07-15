# update-sdk-core Specification

## Purpose
TBD - created by archiving change add-update-sdk-core. Update Purpose after archive.
## Requirements
### Requirement: SDK SHALL be a single zero-platform-dependency npm package

`@swarm-hive/sdk` SHALL be one npm package under `packages/sdk` with subpath exports `"."` (core) and `"./react"`, built to ESM with `.d.ts`. Its `dependencies` SHALL contain only platform-agnostic pure-JS packages (`zustand`, `@noble/hashes`, `semver`); it SHALL NOT depend on `@tauri-apps/*`, `expo-*`, or `react-native`. `react` SHALL be an optional peer dependency used only by `./react`.

#### Scenario: Package has no platform dependencies

- **WHEN** the built package's dependency tree is inspected
- **THEN** no `@tauri-apps/*`, `expo-*`, or `react-native` entry is present
- **AND** both `"."` and `"./react"` subpath imports resolve with their own type declarations

### Requirement: SDK SHALL define the UpdateAdapter ports interface

The package SHALL export an `UpdateAdapter` interface with `check`, `download(onProgress)`, `install`, `storage: KeyValueStorage`, and `compare` members. The engine SHALL depend only on this interface, never on any platform API directly. This interface is the sole contract between the npm package and the platform adapters that live in the registries.

#### Scenario: Engine drives any conforming adapter

- **GIVEN** an in-memory mock adapter implementing `UpdateAdapter`
- **WHEN** the engine runs a full check → download → install cycle
- **THEN** every platform interaction goes through the adapter's methods
- **AND** the engine references no `@tauri-apps`/`expo` symbol

### Requirement: createUpdateEngine SHALL implement the 8-state machine

`createUpdateEngine(adapter, opts)` SHALL expose a framework-agnostic store with `status` ∈ {`idle`, `checking`, `up-to-date`, `available`, `force-required`, `downloading`, `ready`, `error`} and actions `check`, `download`, `install`, `postpone`, `retry`, `acknowledgeError`. A check that finds a newer version SHALL move to `available` (or `force-required` when the update is forced); no newer version SHALL move to `up-to-date`; a download error SHALL move to `error` with a retry path back to `checking`.

#### Scenario: Check yields available

- **GIVEN** an adapter whose `check` returns a release newer than current and `upgradeType="prompt"`
- **WHEN** `engine.check()` runs
- **THEN** `status` transitions `idle → checking → available`
- **AND** `release` holds the returned `ReleaseInfo`

#### Scenario: Forced update yields force-required

- **GIVEN** an adapter whose `check` returns a release with `upgradeType="force"`
- **WHEN** `engine.check()` runs
- **THEN** `status` ends at `force-required`

#### Scenario: Download error is retryable

- **GIVEN** the engine is `available` and the adapter's `download` rejects
- **WHEN** `engine.download()` runs then `engine.retry()`
- **THEN** `status` goes `downloading → error → checking`

### Requirement: SDK SHALL provide pluggable version comparators

The package SHALL export `semverComparator` (single leading `v` tolerated, `semver`-based) and `versionCodeComparator` (integer `versionCode`) for adapters to supply as `adapter.compare`. The engine SHALL determine "is there an update" solely through `adapter.compare`, never with a hardcoded scheme.

#### Scenario: SemVer comparator matches server口径

- **WHEN** `semverComparator` compares current `0.4.0` (or `v0.4.0`) against candidate `0.4.5`
- **THEN** it reports the candidate as newer

#### Scenario: versionCode comparator uses integers

- **WHEN** `versionCodeComparator` compares current `18` against candidate `21`
- **THEN** it reports the candidate as newer, and `21` vs `21` as not newer

### Requirement: Rollout bucketing SHALL match the server algorithm bit-for-bit

The package SHALL export `inRolloutBucket(clientId, percent)` computing `blake3(utf8(clientId))` first 8 bytes as a little-endian u64, `% 100 < percent`, with `percent >= 100 → true` and `percent <= 0 → false` short-circuits — identical to the server `in_rollout_bucket`. For any given `clientId` and `percent`, the TS result SHALL equal the server's.

#### Scenario: Same sample yields same buckets as server

- **GIVEN** the same `client_id` sample set used by the server's rollout smoke test and a fixed `percent`
- **WHEN** `inRolloutBucket` is evaluated for each id in TS
- **THEN** the in-bucket / out-of-bucket partition matches the server's exactly

#### Scenario: Rollout boundaries short-circuit

- **WHEN** `percent` is `100` (or higher) the result is always `true`; **WHEN** `percent` is `0` (or lower) it is always `false`

### Requirement: checkUpdate SHALL parse the Tauri dynamic response

The package SHALL export `checkUpdate` that GETs `/api/v1/updates/tauri/:app_slug` with `current_version`/`target`/`arch`/`channel?`/`client_id?`, returns `null` on `204`, throws `UpdateError` on `4xx/5xx`, and on `200` normalizes the flat JSON (`version`, `pub_date`, `url`, `signature`, `notes`, `swarmhive.{upgrade_type, min_version, rollout_percent, channel}`) into a `ReleaseInfo`. `upgrade_type` SHALL be read directly from `swarmhive.upgrade_type` (no numeric mapping).

#### Scenario: 204 means up-to-date

- **WHEN** the endpoint responds `204 No Content`
- **THEN** `checkUpdate` resolves to `null`

#### Scenario: 200 normalizes flat JSON

- **WHEN** the endpoint responds `200` with a flat `TauriUpdateResponse`
- **THEN** `checkUpdate` returns a `ReleaseInfo` carrying `version`, `url`, `signature`, and `upgradeType` taken from `swarmhive.upgrade_type`

### Requirement: ./react SHALL expose a framework subscription layer

`@swarm-hive/sdk/react` SHALL export `useUpdateEngine(engine)` that subscribes a React component to the engine's state, with `react` as an optional peer dependency. The core (`"."`) SHALL remain usable without React.

#### Scenario: Core works without React

- **WHEN** only `@swarm-hive/sdk` (`"."`) is imported in a non-React context
- **THEN** the engine and all pure helpers work without `react` installed

### Requirement: Types SHALL be generated from the server OpenAPI

The SDK SHALL generate its update-related TS types (`TauriUpdateResponse`, `TauriUpdateExtensions`, `AndroidUpdateResponse`, `UpgradeType`, …) from the server OpenAPI document via `openapi-typescript` (same chain as the admin SPA), so the wire contract has a single source of truth. The SDK package SHALL own its generated schema module independently of the admin SPA's generated client.

#### Scenario: Codegen produces the Tauri wire types

- **WHEN** the SDK codegen script runs against the server OpenAPI doc
- **THEN** a generated schema module exposes `TauriUpdateResponse` matching the server's
  `/api/v1/updates/tauri/:app_slug` 200 body

#### Scenario: Codegen produces the Android wire types

- **WHEN** the SDK codegen script runs against the server OpenAPI doc
- **THEN** the generated schema module exposes `AndroidUpdateResponse` matching the server's
  `/api/v1/updates/android/:app_slug` 200 body

### Requirement: checkUpdateAndroid SHALL parse the RN Android dynamic response

The package SHALL export `checkUpdateAndroid(opts)` that GETs
`/api/v1/updates/android/:app_slug` with query
`current_version_code`/`current_version_name`/`abi?`/`channel?`/`client_id?`/`runtime_version?`,
and normalizes the flat `AndroidUpdateResponse` into a `ReleaseInfo` — or `null` when the body's
`has_update` is `false`. Unlike the Tauri endpoint, no-update SHALL be signaled by `has_update:false`
inside a `200` body, NOT by a `204`. It SHALL throw `UpdateError` with phase `"check"` on `4xx/5xx`
(including the `400` returned for an unparseable `current_version_code`). The normalization SHALL map
`version` ← `version_name`, `versionCode` ← `version_code`, `url` ← `download_url`,
`signature` ← `sha256`, `notes` ← `release_notes`, `upgradeType` ← `upgrade_type` (unknown enum
values falling back to `"prompt"`), `minVersion` ← `String(min_version_code)` when present, and SHALL
set `kind` to `"native-package"`. Because the endpoint does not echo the resolved channel name, the
result's `channel` SHALL be filled from the requested channel (defaulting to `"default"`).

#### Scenario: has_update false yields null

- **WHEN** the endpoint responds `200` with `{ "has_update": false }`
- **THEN** `checkUpdateAndroid` resolves to `null`
- **AND** no `download_url` is required to be present

#### Scenario: 200 with an update normalizes the flat JSON

- **WHEN** the endpoint responds `200` with `has_update:true` carrying `version_name`, `version_code`,
  `download_url`, `sha256`, and `upgrade_type`
- **THEN** `checkUpdateAndroid` returns a `ReleaseInfo` whose `version`, `versionCode`, `url`,
  `signature`, and `upgradeType` are taken from those wire fields
- **AND** the returned `kind` is `"native-package"`

#### Scenario: unparseable version code surfaces as a check error

- **WHEN** the endpoint responds `400` because `current_version_code` could not be parsed
- **THEN** `checkUpdateAndroid` throws `UpdateError` with phase `"check"`

#### Scenario: runtime_version is forwarded but does not affect MVP results

- **WHEN** `opts.runtimeVersion` is supplied
- **THEN** it is sent as the `runtime_version` query parameter
- **AND** the normalized `ReleaseInfo` is unchanged versus omitting it (server ignores it in MVP)

### Requirement: ReleaseInfo SHALL carry an optional OTA kind discriminant

`ReleaseInfo` SHALL include an optional `kind?: "native-package" | "ota-bundle"` field. Absence of
`kind` SHALL be treated as `"native-package"`; only an explicit `"ota-bundle"` denotes an OTA bundle.
All MVP check paths SHALL produce a native package: `checkUpdateAndroid` SHALL set
`kind="native-package"` and `checkUpdate` (Tauri) MAY leave it absent (interpreted as native-package).
No MVP code path SHALL produce `"ota-bundle"` — that value is reserved for the future OTA provider.
The field SHALL be purely additive: consumers that ignore it are unaffected.

#### Scenario: Android normalization sets native-package

- **WHEN** `checkUpdateAndroid` returns a `ReleaseInfo`
- **THEN** its `kind` is `"native-package"`

#### Scenario: Absent kind is interpreted as native-package

- **GIVEN** a `ReleaseInfo` with no `kind` (e.g. from the Tauri `checkUpdate` path)
- **WHEN** a consumer checks whether it is an OTA bundle via `release.kind === "ota-bundle"`
- **THEN** the check is `false`

### Requirement: ReleaseInfo SHALL carry mirror candidates and RN download SHALL fail over across sources

The SDK's `ReleaseInfo` SHALL carry an optional ordered list of mirror download URLs, and `normalizeAndroid` SHALL populate it from the RN update response's `mirror_urls`. `ReleaseInfo` SHALL also carry an optional `sizeBytes`, populated by `normalizeAndroid` from the RN update response's `size_bytes`, so that the downloader can reject a truncated delivery. The reference RN adapter's `download()` SHALL attempt the primary `url` first and, on a download failure, fall through to the mirror candidates in order until one succeeds or all are exhausted. When every source fails, the adapter SHALL surface a retryable download error (not a silent success).

A "download failure" that triggers fall-through SHALL be whatever the injected downloader rejects with. The downloader — NOT the adapter — SHALL own delivery validation, so that the adapter remains pure, injectable logic with no dependency on `expo-*` (see the registry-rn downloader-validation requirement for what it MUST reject). The adapter SHALL pass the expected `sizeBytes` through to the downloader.

Client-side `sha256` verification SHALL NOT be required, superseding the earlier requirement that a post-download `sha256` mismatch trigger fall-through. That requirement was never implementable at acceptable cost and was redundant with an existing server-side gate:

- **No cheap path exists.** `expo-file-system` exposes only md5 (native, streaming); `expo-crypto`'s `digestStringAsync` is one-shot and requires the entire APK as a JS string (a 50MB APK ≈ 67MB base64 in the JS heap); a native streaming-hash dependency would violate registry-rn's established no-native-code constraint.
- **The server already does it, better.** The `github-release-source` capability requires the server to verify a mirror's digest against `artifact.sha256` BEFORE exposing it as a candidate — once, cached, single-flighted. The "same-repo but wrong-bytes asset" scenario the old requirement targeted is that gate's exact purpose; such an asset never becomes a candidate.
- **Residual risk is covered.** Transport corruption is caught by the size and ZIP-magic checks; APK authenticity is enforced by Android's PackageInstaller signature verification, which is the real integrity gate.

#### Scenario: Primary fails, mirror succeeds

- **GIVEN** a `ReleaseInfo` whose primary `url` returns an error page and whose first mirror serves the correct APK
- **WHEN** the RN adapter downloads
- **THEN** it falls through to the mirror and completes the install

#### Scenario: All sources fail surfaces a retryable error

- **GIVEN** a primary and all mirrors failing to deliver a valid APK
- **WHEN** the RN adapter downloads
- **THEN** it surfaces a retryable download error, not a silent success

#### Scenario: No mirrors preserves single-source behavior

- **GIVEN** a `ReleaseInfo` with an empty mirror list
- **WHEN** the RN adapter downloads
- **THEN** it behaves exactly as the pre-existing single-`url` flow

#### Scenario: Expected size reaches the downloader

- **GIVEN** a `ReleaseInfo` normalized from a response carrying `size_bytes`
- **WHEN** the RN adapter downloads
- **THEN** it passes the expected `sizeBytes` to the injected downloader, which owns the validation

#### Scenario: Adapter stays free of expo dependencies

- **GIVEN** the adapter's download path
- **WHEN** its unit tests run
- **THEN** they exercise it with a fake downloader and no `expo-*` module is required, because validation lives in the downloader

