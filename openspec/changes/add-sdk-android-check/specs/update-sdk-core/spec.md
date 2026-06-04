## ADDED Requirements

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

## MODIFIED Requirements

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
