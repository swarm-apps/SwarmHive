# web-artifact-upload Specification

## Purpose
TBD - created by archiving change add-web-artifact-upload. Update Purpose after archive.
## Requirements
### Requirement: Admin SHALL upload artifacts via browser presign direct upload

The Admin SPA SHALL let a user with `artifact:upload` add artifacts to a release from `ArtifactsDrawer`: select/drag one or more files, then for each file compute hex MD5 + SHA256 client-side, call `presign`, `PUT` each file directly to object storage replaying the returned headers (including `Content-MD5`) while showing per-file progress, then call `complete`. The flow SHALL NOT proxy bytes through the server. Hashing SHALL run off the main thread so the UI stays responsive for large (hundreds-of-MB) files.

#### Scenario: Drag-and-drop upload to a draft release

- **WHEN** the user drops a `.msi` and a `.apk` into the drawer and confirms upload
- **THEN** each file's MD5 + SHA256 are computed in a Web Worker without freezing the UI
- **AND** the client requests `presign`, then `PUT`s each file to storage with the presigned headers and a visible progress bar
- **AND** the client calls `complete`, and the new artifacts appear in the drawer

#### Scenario: Hashing does not block the UI

- **GIVEN** a multi-hundred-MB artifact selected for upload
- **WHEN** its hash is being computed
- **THEN** hashing happens in a Web Worker (streaming via chunked `Blob.slice`)
- **AND** the drawer remains interactive with progress feedback

### Requirement: Admin SHALL classify platform metadata from filenames

The Admin SPA SHALL infer `platform` / `target` / `abi` from each filename **in the drag-and-drop batch mode** and present them as editable fields before upload. `.apk` SHALL map to `react-native-android` (deriving `abi` from a recognized substring such as `arm64-v8a`); desktop bundle extensions (`.msi`, `.exe`, `.dmg`, `.app.tar.gz`, `.AppImage`, `.deb`, `.rpm`, `.nsis.zip`) SHALL map to `tauri-desktop`. Unrecognized extensions SHALL default to `tauri-desktop` flagged for user confirmation. The user SHALL be able to override any inferred value. The guided per-platform form does **not** rely on filename inference — there the user picks platform / target / abi explicitly.

#### Scenario: APK is classified as Android with ABI

- **WHEN** a file named `app-arm64-v8a-release.apk` is added in batch mode
- **THEN** its inferred platform is `react-native-android` and `abi` is `arm64-v8a`
- **AND** the user can edit these before uploading

#### Scenario: Unknown extension is flagged

- **WHEN** a file with an unrecognized extension is added in batch mode
- **THEN** it defaults to `tauri-desktop` and is visibly flagged for the user to confirm or change

### Requirement: Admin SHALL pair Tauri .sig signatures with their bundle

The Admin SPA SHALL treat a dropped `.sig` file as the signature of the sibling bundle whose name equals the `.sig` name minus the `.sig` suffix. The `.sig` SHALL NOT be uploaded as its own artifact; its text content SHALL be sent as the matching bundle part's `signature` at `complete`. A `.sig` with no matching bundle SHALL surface an explicit error rather than being silently dropped.

#### Scenario: .sig is paired and sent inline

- **WHEN** the user drops `Foo.app.tar.gz` and `Foo.app.tar.gz.sig`
- **THEN** only `Foo.app.tar.gz` is uploaded as an artifact
- **AND** the `.sig` text is attached as that part's `signature` in `complete`

#### Scenario: Orphan .sig is rejected

- **WHEN** the user drops a `.sig` with no matching bundle in the batch
- **THEN** the drawer shows an error and does not start the upload

### Requirement: Admin SHALL optionally publish and promote after upload

The upload flow SHALL offer a publish toggle (passed as `complete`'s `publish`) and, for a user holding `release:promote`, an optional channel selection that promotes the chosen channel to this release after a successful publish. Controls the user lacks permission for SHALL be hidden, consistent with the hide-not-disable policy.

#### Scenario: Upload, publish, and promote stable in one flow

- **GIVEN** a user holding `artifact:upload` + `release:publish` + `release:promote`
- **WHEN** they upload with publish enabled and select the `stable` channel
- **THEN** `complete` publishes the release
- **AND** `stable` is promoted to this release's version

#### Scenario: Promote control hidden without permission

- **GIVEN** a user lacking `release:promote`
- **WHEN** they open the upload flow
- **THEN** the channel-promote control is not shown

### Requirement: Admin SHALL offer one-click CORS configuration on the storage page

The storage page SHALL provide a "configure CORS" action on a backend that calls `POST /storage/backends/:id/cors` with the admin's own origin (`window.location.origin`). A successful result SHALL be surfaced as success; a fallback result (`ok=false`) SHALL surface the returned guidance directing manual configuration. The action SHALL be visible only with `storage:manage`.

#### Scenario: Configure CORS from the storage page

- **GIVEN** a user holding `storage:manage` on the storage page
- **WHEN** they click "configure CORS" on a backend
- **THEN** the request sends `{ allowed_origins: [current origin] }`
- **AND** a success notification confirms CORS is configured

#### Scenario: Manual-fallback guidance on unsupported backend

- **WHEN** the backend returns `{ ok: false, detail }`
- **THEN** the page surfaces `detail` guiding the user to configure CORS manually

### Requirement: Admin SHALL offer a guided per-platform upload form

On the **release detail page** the 「上传产物」 action SHALL open a **centered Modal**（no longer a drawer）offering a guided upload mode as the primary path: the user first picks the platform（Tauri desktop / React Native Android）, then a platform-specific form is shown — **Tauri** exposes a `target` selector（target triple, friendly label）plus an optional `.sig` signature input; **Android** exposes an `abi` selector（versionCode is a release-level field, only hinted）— and uploads the corresponding package. The guided form SHALL reuse the existing hash-worker / presign / direct-PUT / complete pipeline unchanged. The drag-and-drop batch mode（filename classification）SHALL remain available as an alternative path within the same Modal.

#### Scenario: Guided Tauri upload from the detail-page Modal

- **WHEN** the user opens 「上传产物」 on the release detail page, picks "Tauri desktop", selects target `aarch64-apple-darwin`, and attaches a `.dmg`（optionally its `.sig`）
- **THEN** the Modal uploads the artifact as `tauri-desktop` / `aarch64-apple-darwin`, the `.sig` rides along at `complete`, and the artifacts table refreshes

#### Scenario: Batch mode still available in the Modal

- **WHEN** the user switches to drag-and-drop mode inside the upload Modal and drops multiple files
- **THEN** filename classification infers platform/target/abi as before

