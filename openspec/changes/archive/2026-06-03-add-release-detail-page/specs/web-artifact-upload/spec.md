## MODIFIED Requirements

### Requirement: Admin SHALL offer a guided per-platform upload form

On the **release detail page** the 「上传产物」 action SHALL open a **centered Modal**（no longer a drawer）offering a guided upload mode as the primary path: the user first picks the platform（Tauri desktop / React Native Android）, then a platform-specific form is shown — **Tauri** exposes a `target` selector（target triple, friendly label）plus an optional `.sig` signature input; **Android** exposes an `abi` selector（versionCode is a release-level field, only hinted）— and uploads the corresponding package. The guided form SHALL reuse the existing hash-worker / presign / direct-PUT / complete pipeline unchanged. The drag-and-drop batch mode（filename classification）SHALL remain available as an alternative path within the same Modal.

#### Scenario: Guided Tauri upload from the detail-page Modal

- **WHEN** the user opens 「上传产物」 on the release detail page, picks "Tauri desktop", selects target `aarch64-apple-darwin`, and attaches a `.dmg`（optionally its `.sig`）
- **THEN** the Modal uploads the artifact as `tauri-desktop` / `aarch64-apple-darwin`, the `.sig` rides along at `complete`, and the artifacts table refreshes

#### Scenario: Batch mode still available in the Modal

- **WHEN** the user switches to drag-and-drop mode inside the upload Modal and drops multiple files
- **THEN** filename classification infers platform/target/abi as before
