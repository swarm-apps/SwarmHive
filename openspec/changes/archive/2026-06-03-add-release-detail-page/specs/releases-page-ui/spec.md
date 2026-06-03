## ADDED Requirements

### Requirement: Admin SHALL open a release detail page

A release row's enter / 「产物」 action SHALL navigate to a **release detail page** at `/apps/:slug/releases/:version`（rendered inside the version tab's outlet）. The page SHALL show the release's metadata（version, status Tag, published time, release notes）with header actions（上传产物 / 编辑 / 发布 / 撤回, permission-gated）, its artifacts table as the body, and a breadcrumb 「应用 / <slug> / 版本 / <version>」. The page SHALL be deep-linkable.

#### Scenario: Entering a release opens its detail page

- **WHEN** the user activates a release row's enter / 「产物」 action for version `0.4.0`
- **THEN** the app navigates to `/apps/swarmnote/releases/0.4.0`
- **AND** the page shows the release metadata, header actions, and the artifacts table

#### Scenario: Release detail deep link resolves

- **WHEN** the user opens `/apps/swarmnote/releases/0.4.0` directly
- **THEN** the release detail page renders without first visiting the version list

## MODIFIED Requirements

### Requirement: Admin SHALL view a release's artifacts read-only

The release's artifacts SHALL be presented on the **release detail page**（`/apps/:slug/releases/:version`, no longer in a drawer）as a flat ProTable（one row per artifact）backed by `GET /api/v1/apps/:slug/releases/:version/artifacts`, with columns: platform（merged via `rowSpan` over consecutive same-platform rows）, architecture（friendly label from target triple）, filename, size（right-aligned）, sha256（truncated + copy）, and signature state（status Tag）. An `expandable` row SHALL reveal full sha256, signature, and upload time. Upload is offered via a separate Modal on this page（not inline in the table).

#### Scenario: Artifacts table renders on the detail page

- **GIVEN** a release with a `tauri-desktop` and a `react-native-android` artifact
- **WHEN** the user is on `/apps/:slug/releases/:version`
- **THEN** a ProTable shows each artifact's friendly architecture, filename, size, copyable sha256, and signature Tag
- **AND** consecutive same-platform rows share one merged platform cell
