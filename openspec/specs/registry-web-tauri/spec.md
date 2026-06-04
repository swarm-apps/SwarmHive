# registry-web-tauri Specification

## Purpose
TBD - created by archiving change add-registry-web-tauri. Update Purpose after archive.
## Requirements
### Requirement: registry-web SHALL be a shadcn registry source package

`packages/registry-web` SHALL contain a `registry.json` catalog plus `registry/tauri/<item>/` sources, and a `build:registry` script (`shadcn build`) producing `public/r/registry.json` and per-item `public/r/<name>.json`. Items SHALL declare npm `dependencies` (`@swarm-hive/sdk`, `@tauri-apps/*`, `@radix-ui/*`, `lucide-react`) and chain each other via `registryDependencies`.

#### Scenario: build produces flat registry JSON

- **WHEN** `pnpm --filter @swarm-hive/registry-web build:registry` runs
- **THEN** `public/r/registry.json` and a JSON per item are produced
- **AND** each item's `files[].content` is inlined from source

### Requirement: tauriAdapter SHALL implement UpdateAdapter via plugin-updater

The `tauriAdapter` SHALL implement the SDK `UpdateAdapter`. Its `check` SHALL call `@tauri-apps/plugin-updater`'s `check()` (which performs minisign verification), cache the returned `Update` in a closure, and normalize `update.rawJson.swarmhive` into a SDK `ReleaseInfo`. It SHALL NOT use the SDK `checkUpdate` (that is for the RN adapter). The SDK `UpdateAdapter` interface SHALL remain unchanged — the platform `Update` lives only inside the adapter.

#### Scenario: check normalizes from rawJson.swarmhive

- **GIVEN** plugin-updater `check()` returns an `Update` whose `rawJson.swarmhive.upgrade_type="force"`
- **WHEN** `tauriAdapter.check` runs
- **THEN** it returns a `ReleaseInfo` with `upgradeType="force"` and `version`/`url`/`signature` derived from the `Update`
- **AND** the `Update` is cached for the subsequent `download`

### Requirement: tauriAdapter SHALL implement download and install separately

plugin-updater's `Update` exposes standalone `download(onEvent?)` and `install()`. The adapter's `download` SHALL call the cached `Update.download(onEvent)` and map `DownloadEvent` (`Started`/`Progress`/`Finished`) to the SDK `Progress` (via a 500ms-throttled speed tracker living in the adapter, not sdk-core); `install` SHALL call `Update.install()` then `relaunch()`.

#### Scenario: download events map to progress

- **GIVEN** a cached `Update`
- **WHEN** `download` runs and `Update.download` emits `Started{contentLength}` then `Progress{chunkLength}`
- **THEN** the `onProgress` callback receives a `Progress` with a computed `percent`

#### Scenario: install installs then relaunches

- **WHEN** `install` runs after a successful download
- **THEN** the adapter calls `Update.install()` then `relaunch()`

### Requirement: tauriAdapter SHALL pass client_id via header for server-side rollout

Because plugin-updater's `check()` supports runtime `headers` but not custom query params, the adapter SHALL call `check({ headers: { 'X-Client-Id': clientId } })`, and the server's `/api/v1/updates/tauri/:app_slug` endpoint SHALL read `client_id` from the `X-Client-Id` header (falling back to the query param, then request IP). Rollout bucketing thus takes effect server-side for Tauri. The SDK's `inRolloutBucket` SHALL remain exported as optional client-side defense-in-depth.

#### Scenario: server reads X-Client-Id header for bucketing

- **GIVEN** the served release has `rolloutPercent=50`
- **WHEN** a request hits `/api/v1/updates/tauri/:slug` with an `X-Client-Id` header and no query `client_id`
- **THEN** the server buckets by the header value (200 if in bucket, 204 otherwise)

#### Scenario: adapter sends the header

- **WHEN** `tauriAdapter.check` runs
- **THEN** it calls plugin-updater `check` with `headers["X-Client-Id"]` set to the engine's `clientId`

#### Scenario: full rollout always served

- **GIVEN** `rolloutPercent` is 100 (or absent)
- **WHEN** an update is available
- **THEN** the server returns it (rollout never blocks)

### Requirement: registryDependencies SHALL chain components to hook and adapter via namespace

A UI component item SHALL list `use-update` in its `registryDependencies`, and `use-update` SHALL list `tauri-adapter`, so installing a single component transitively installs the hook and adapter. References SHALL use the **namespace form `@swarmhive/<name>`** (not a hardcoded host); the user maps `@swarmhive` in `components.json` to a GitHub raw URL (e.g. `https://raw.githubusercontent.com/swarm-apps/swarmhive/<ref>/packages/registry-web/public/r/{name}.json`).

#### Scenario: installing a dialog pulls the chain

- **GIVEN** the user has mapped `@swarmhive` in `components.json` to the GitHub raw URL
- **WHEN** the user runs `shadcn add @swarmhive/prompt-update-dialog`
- **THEN** `use-update` and `tauri-adapter` are also installed (resolved via the same `@swarmhive` namespace)
- **AND** their npm `dependencies` (`@swarm-hive/sdk`, `@tauri-apps/*`, `@radix-ui/*`, `lucide-react`) are installed

### Requirement: registry build output SHALL be committed for GitHub raw distribution

`shadcn build` output (`packages/registry-web/public/r/*.json`) SHALL be committed to the repository, because GitHub raw distribution serves those files directly. The server SHALL NOT host registry JSON — there is no `/r` endpoint, no `rust-embed` of registry files.

#### Scenario: built registry JSON is committed

- **WHEN** `pnpm --filter @swarm-hive/registry-web build:registry` runs
- **THEN** `public/r/registry.json` and per-item JSON are produced under the package
- **AND** these files are tracked in git (so a GitHub raw URL resolves them)

### Requirement: UI components SHALL consume the SDK engine without duplicating logic

registry-web UI SHALL drive state through `useUpdate` (`= useUpdateEngine(createUpdateEngine(tauriAdapter))`); it SHALL NOT reimplement the state machine, comparator, or rollout — those come from `@swarm-hive/sdk`.

#### Scenario: no sdk logic duplication

- **WHEN** the registry-web sources are inspected
- **THEN** the 8-state machine, version comparison, and rollout bucketing are imported from `@swarm-hive/sdk`, not reimplemented in registry-web

