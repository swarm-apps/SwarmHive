# update-sdk-core

## ADDED Requirements

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

The update-related TS types (`TauriUpdateResponse`, `TauriUpdateExtensions`, `UpgradeType`, …) SHALL be generated from the server OpenAPI document via `openapi-typescript` (same chain as the admin SPA), so the wire contract has a single source of truth.

#### Scenario: Codegen produces the wire types

- **WHEN** the SDK codegen script runs against the server OpenAPI doc
- **THEN** a generated schema module exposes `TauriUpdateResponse` matching the server's `/api/v1/updates/tauri/:app_slug` 200 body
