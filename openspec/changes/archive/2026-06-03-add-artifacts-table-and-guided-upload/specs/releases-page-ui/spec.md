## MODIFIED Requirements

### Requirement: Admin SHALL view a release's artifacts read-only

From a row action the 版本 tab SHALL open a view backed by `GET /api/v1/apps/:slug/releases/:version/artifacts` presenting artifacts as a **flat ProTable**（one row per artifact）with columns: **platform**（merged via `rowSpan` over consecutive same-platform rows; rows pre-sorted by platform）, **architecture**（a friendly label derived from the target triple — e.g. `aarch64-apple-darwin` → "macOS Apple Silicon", `x86_64-pc-windows-msvc` → "Windows x64"; `abi` kept as-is）, **filename**, **size**（right-aligned）, **sha256**（truncated with a one-click copy affordance, never rendered full for visual comparison）, and **signature state**（a status Tag: signed when `signature_metadata` is present, else unsigned）. An `expandable` row SHALL reveal the full sha256（copyable）, the full minisign signature, upload time, and download count. No upload or delete is offered in this read view（upload is a separate guided/batch affordance）.

#### Scenario: Artifacts table lists binaries with merged platform and friendly arch

- **GIVEN** a release with a `tauri-desktop` / `aarch64-apple-darwin` artifact and a `react-native-android` / `arm64-v8a` artifact
- **WHEN** the user opens its artifacts view
- **THEN** a table shows each artifact's friendly architecture, filename, size, truncated-copyable sha256, and a signature Tag
- **AND** consecutive same-platform rows share one merged platform cell

#### Scenario: Expanded row reveals full checksum and signature

- **WHEN** the user expands an artifact row
- **THEN** the full sha256（copyable）and the signature（when present）are shown
