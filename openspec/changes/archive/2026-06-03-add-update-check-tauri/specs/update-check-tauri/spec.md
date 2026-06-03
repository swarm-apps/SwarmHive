# update-check-tauri

## ADDED Requirements

### Requirement: Server SHALL expose a Tauri v2 dynamic update endpoint

The server SHALL expose a public, unauthenticated `GET /api/v1/updates/tauri/:app_slug` accepting query params `current_version` (required, SemVer, a single leading `v` tolerated), `target` (required, OS name `darwin`/`windows`/`linux`), `arch` (required, `x86_64`/`aarch64`/`i686`/`armv7`), `channel` (optional, defaults to the app's `is_default` channel), and `client_id` (optional). When an update is available the response SHALL be `200` with a flat JSON body `{ version, pub_date?, url, signature, notes?, swarmhive }`. When no update is available the response SHALL be `204 No Content` with an empty body. The endpoint SHALL NOT be rate-limited by the governor layer.

#### Scenario: Update available returns 200 with flat JSON

- **GIVEN** an app `swarmdrop` whose default channel `stable` serves a published release `0.4.5` with a matching `tauri-desktop` artifact carrying a `tauri_signature`
- **WHEN** a client GETs `/api/v1/updates/tauri/swarmdrop?current_version=0.4.0&target=darwin&arch=aarch64`
- **THEN** the response is `200` with `Content-Type: application/json`
- **AND** the body has top-level `version="0.4.5"`, a `url` pointing at `/download/swarmdrop/0.4.5/<artifact_id>`, and a non-empty `signature`
- **AND** the body has `swarmhive.channel="stable"` and `swarmhive.upgrade_type="prompt"`

#### Scenario: Leading v on current_version is tolerated

- **GIVEN** the default channel serves published release `0.4.5`
- **WHEN** a client GETs the endpoint with `current_version=v0.4.0`
- **THEN** the single leading `v` is stripped before comparison and the response is `200` (an update is offered)

#### Scenario: No newer version returns 204

- **GIVEN** the default channel serves published release `0.4.5`
- **WHEN** a client GETs the endpoint with `current_version=0.4.5` (equal) or `current_version=0.5.0` (higher)
- **THEN** the response is `204 No Content` with an empty body

#### Scenario: Channel never promoted returns 204

- **GIVEN** an app whose default channel has no `channel_release` pointer
- **WHEN** a client checks for updates on that channel
- **THEN** the response is `204 No Content`

#### Scenario: No default channel returns 204

- **GIVEN** an app where no channel has `is_default=true` (e.g. an operator unset the default)
- **WHEN** a client checks for updates without specifying `channel`
- **THEN** the response is `204 No Content` (the handler treats a missing default channel as "nothing to serve", never panics)

#### Scenario: Draft or yanked target release returns 204

- **GIVEN** the default channel pointer references a release whose status is `draft` or `yanked`
- **WHEN** a client checks for updates
- **THEN** the response is `204 No Content`

#### Scenario: Unknown app slug returns 404

- **WHEN** a client GETs `/api/v1/updates/tauri/does-not-exist?current_version=1.0.0&target=darwin&arch=aarch64`
- **THEN** the response is `404`

#### Scenario: Explicitly named missing channel returns 404

- **GIVEN** an app with channels `dev`/`beta`/`stable`
- **WHEN** a client GETs the endpoint with `channel=nightly`
- **THEN** the response is `404`

#### Scenario: Malformed current_version returns 400

- **WHEN** a client GETs the endpoint with `current_version=not-a-version`
- **THEN** the response is `400` with `type` `.../errors/invalid-current-version`

### Requirement: Server SHALL match artifacts by parsing Rust target triples

The server SHALL select the artifact for a `(target, arch)` request by parsing each `tauri-desktop` artifact's stored Rust target triple (`<arch>-<vendor>-<sys>`) into an `(os, arch)` pair and comparing against the request, considering only artifacts that carry a `tauri_signature`. An exact `(os, arch)` match SHALL take priority. A `universal-apple-darwin` artifact SHALL match any `arch` when `target=darwin`. If no match exists but the release has exactly one `tauri-desktop` artifact whose `target` is NULL, the server SHALL fall back to that artifact. Otherwise the server SHALL return `204`.

#### Scenario: Exact target/arch match across multiple artifacts

- **GIVEN** a release with `tauri-desktop` artifacts targeting `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`
- **WHEN** a client requests `target=darwin&arch=aarch64`
- **THEN** the `url` resolves to the `aarch64-apple-darwin` artifact, not the Windows one

#### Scenario: Universal macOS binary matches any arch

- **GIVEN** a release whose only `tauri-desktop` artifact targets `universal-apple-darwin`
- **WHEN** a client requests `target=darwin&arch=x86_64` or `target=darwin&arch=aarch64`
- **THEN** that universal artifact is returned in both cases

#### Scenario: Single untargeted artifact fallback

- **GIVEN** a release with exactly one `tauri-desktop` artifact whose `target` is NULL
- **WHEN** a client requests any `target`/`arch`
- **THEN** that single artifact is returned

#### Scenario: No matching platform returns 204

- **GIVEN** a release whose only `tauri-desktop` artifact targets `aarch64-apple-darwin`
- **WHEN** a client requests `target=windows&arch=x86_64`
- **THEN** the response is `204 No Content`

#### Scenario: Published release with zero Tauri artifacts returns 204

- **GIVEN** a published release the default channel serves that has no `tauri-desktop` artifact (e.g. an Android-only or not-yet-uploaded release)
- **WHEN** a client checks for updates
- **THEN** the response is `204 No Content` (the handler never panics on an empty artifact set)

#### Scenario: Matching artifact without signature returns 204

- **GIVEN** the matching `tauri-desktop` artifact has no `tauri_signature` in `signature_metadata`
- **WHEN** a client checks for updates
- **THEN** the response is `204 No Content` (an unsigned update would fail client-side verification)

### Requirement: Server SHALL gate updates on rollout bucketing

When the served release has `rollout_percent` below 100, the server SHALL deterministically bucket the request using `blake3` of the `client_id` (or the request IP when `client_id` is absent) and return `204` for requests outside the rollout bucket. When `client_id` and IP are both absent the server SHALL treat the request as in-bucket AND emit a `tracing::warn` that bucketing was bypassed. When `rollout_percent` is NULL or 100 the server SHALL skip bucketing entirely. Operators of direct (non-proxied) single-server deployments are therefore advised that gradual rollout only takes effect when the SDK supplies `client_id`.

#### Scenario: Client outside the rollout bucket gets 204

- **GIVEN** the served release has `rollout_percent=50`
- **WHEN** many distinct `client_id`s check for updates
- **THEN** the share receiving `200` falls within a tolerant band around 50% (e.g. `[40%, 60%]` for a few-hundred sample, avoiding statistical flakiness)
- **AND** a given `client_id` deterministically gets the same result on repeat calls

#### Scenario: Full rollout skips bucketing

- **GIVEN** the served release has `rollout_percent=100` (or NULL)
- **WHEN** any client checks for updates
- **THEN** rollout never causes a `204` (the update is offered to everyone)

#### Scenario: No client_id and no IP bypasses bucketing observably

- **GIVEN** the served release has `rollout_percent=50` and the request carries neither `client_id` nor a forwarded IP
- **WHEN** the client checks for updates
- **THEN** the request is treated as in-bucket (`200` if otherwise eligible)
- **AND** a `tracing::warn` records that rollout bucketing was bypassed

### Requirement: Server SHALL signal forced updates via min_version

When the served release has a non-NULL `min_version` and `min_version > current_version` (SemVer, a single leading `v` tolerated on both sides), the server SHALL set `swarmhive.upgrade_type="force"`; otherwise `"prompt"`. The `swarmhive` extension object SHALL also carry `min_version`, `rollout_percent`, and `channel`. Custom fields SHALL live under the `swarmhive` namespace so they do not collide with Tauri's standard fields.

#### Scenario: min_version above current triggers force

- **GIVEN** the served release `0.5.0` has `min_version=0.4.0`
- **WHEN** a client with `current_version=0.3.0` checks for updates
- **THEN** the response is `200` with `swarmhive.upgrade_type="force"` and `swarmhive.min_version="0.4.0"`

#### Scenario: current at or above min_version is a prompt

- **GIVEN** the served release `0.5.0` has `min_version=0.4.0`
- **WHEN** a client with `current_version=0.4.2` checks for updates
- **THEN** the response is `200` with `swarmhive.upgrade_type="prompt"`

### Requirement: Server SHALL emit update telemetry events

On every request the server SHALL emit a structured `update_check` tracing event; when an update is offered (`200`) it SHALL additionally emit `update_available`. Field names SHALL match the `add-telemetry-events` `update_event` columns: `update_check` carries `app_id`, `channel`, `current_version`, `platform="tauri-desktop"`, `target`, `arch`, and `anonymous_client_id` (the wire `client_id`); `update_available` additionally carries `release_id`, `artifact_id`, and `storage_backend_id`. Event emission SHALL NOT block or fail the update response.

#### Scenario: Events are emitted on the hot path

- **WHEN** a client checks for updates and receives `200`
- **THEN** both an `update_check` and an `update_available` event are emitted
- **AND** `anonymous_client_id` holds the request's `client_id` value

### Requirement: Release SHALL carry rollout and forced-update fields

The `release` entity (originally defined by `add-app-release-artifact`) SHALL additionally carry `min_version: Option<String>` (SemVer lower bound for forced update; NULL = none) and `rollout_percent: Option<i16>` (1–100 gradual rollout; NULL = treated as 100). These fields SHALL be settable via `PATCH /api/v1/apps/:slug/releases/:version` and SHALL appear on the `api::Release` DTO. The PATCH handler SHALL reject `rollout_percent` outside `1..=100` and a non-NULL `min_version` that fails SemVer parsing with `422`. Because the request uses single-level `Option` (absent and `null` both mean "unchanged"), the endpoint does NOT support resetting either field back to NULL; operators clear a mistaken value by setting an inert boundary instead (`min_version="0.0.0"` is effectively no lower bound; `rollout_percent=100` is full rollout).

#### Scenario: PATCH sets rollout and min_version

- **GIVEN** a published release `0.5.0`
- **WHEN** an authorized client PATCHes it with `{ min_version: "0.4.0", rollout_percent: 50 }`
- **THEN** the release reflects both values and subsequent update checks honor them

#### Scenario: Invalid rollout_percent is rejected

- **WHEN** an authorized client PATCHes a release with `rollout_percent: 0` or `rollout_percent: 150`
- **THEN** the response is `422` with `type` `.../errors/invalid-rollout-percent`

#### Scenario: Invalid min_version is rejected

- **WHEN** an authorized client PATCHes a release with `min_version: "not-semver"`
- **THEN** the response is `422` with `type` `.../errors/invalid-min-version`

#### Scenario: Mistaken value cleared via boundary

- **GIVEN** a release whose `min_version` was set too high
- **WHEN** an authorized client PATCHes `min_version: "0.0.0"`
- **THEN** subsequent checks always yield `upgrade_type="prompt"` (no client is force-blocked)

#### Scenario: Legacy release with NULL rollout serves at full rollout

- **GIVEN** a release created before this change whose `rollout_percent` is NULL
- **WHEN** a client checks for updates
- **THEN** rollout bucketing is skipped (treated as 100)

### Requirement: Release creation SHALL validate the version is SemVer

`POST /api/v1/apps/:slug/releases` (originally defined by `add-app-release-artifact`) SHALL reject a `version` that is not valid SemVer (a single leading `v` tolerated) with `422` `type` `.../errors/invalid-release-version`, so a release can never be created with a version that the update-check endpoint would later fail to parse and silently skip.

#### Scenario: Non-SemVer release version is rejected at creation

- **WHEN** an authorized client POSTs a release with `version: "latest"` or `version: "1.0"` that fails SemVer parse
- **THEN** the response is `422` with `type` `.../errors/invalid-release-version`
- **AND** no `release` row is created
