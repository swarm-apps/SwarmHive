## ADDED Requirements

### Requirement: Admin SHALL offer a guided per-platform upload form

The `ArtifactsDrawer` SHALL offer a guided upload mode as the primary path: the user first picks the platform (Tauri desktop / React Native Android), then a **platform-specific form** is shown — **Tauri** exposes a `target` selector (target triple, displayed with a friendly label) plus an optional `.sig` signature input; **Android** exposes an `abi` selector plus the release `versionCode` field — and uploads the corresponding package. The guided form SHALL reuse the existing hash-worker / presign / direct-PUT / complete pipeline unchanged. The drag-and-drop batch mode (filename classification) SHALL remain available as an alternative/advanced path.

#### Scenario: Guided Tauri upload carries target and signature

- **WHEN** the user picks platform "Tauri desktop", selects target `aarch64-apple-darwin`, and attaches a `.dmg` (optionally its `.sig`)
- **THEN** the artifact uploads as `tauri-desktop` / `aarch64-apple-darwin`, the `.sig` text rides along at `complete`, and it appears in the artifacts table

#### Scenario: Guided Android upload exposes abi and versionCode

- **WHEN** the user picks platform "Android"
- **THEN** the form shows an `abi` selector and a `versionCode` field (neither shown for Tauri) plus an `.apk` upload

#### Scenario: Drag batch mode remains available

- **WHEN** the user switches to drag-and-drop mode and drops multiple files
- **THEN** filename classification still infers platform/target/abi as before (advanced/batch path)

## MODIFIED Requirements

### Requirement: Admin SHALL classify platform metadata from filenames

The Admin SPA SHALL infer `platform` / `target` / `abi` from each filename **in the drag-and-drop batch mode** and present them as editable fields before upload. `.apk` SHALL map to `react-native-android` (deriving `abi` from a recognized substring such as `arm64-v8a`); desktop bundle extensions (`.msi`, `.exe`, `.dmg`, `.app.tar.gz`, `.AppImage`, `.deb`, `.rpm`, `.nsis.zip`) SHALL map to `tauri-desktop`. Unrecognized extensions SHALL default to `tauri-desktop` flagged for user confirmation. The user SHALL be able to override any inferred value. The guided per-platform form does **not** rely on filename inference — there the user picks platform / target / abi explicitly.

#### Scenario: APK is classified as Android with ABI

- **WHEN** a file named `app-arm64-v8a-release.apk` is added in batch mode
- **THEN** its inferred platform is `react-native-android` and `abi` is `arm64-v8a`
- **AND** the user can edit these before uploading

#### Scenario: Unknown extension is flagged

- **WHEN** a file with an unrecognized extension is added in batch mode
- **THEN** it defaults to `tauri-desktop` and is visibly flagged for the user to confirm or change
