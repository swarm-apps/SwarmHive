# releases-page-ui Specification

## Purpose
TBD - created by archiving change add-releases-page-ui. Update Purpose after archive.
## Requirements
### Requirement: Admin SHALL list releases with lifecycle state

For the app in scope (from the `/apps/:slug` detail route's path param, not a `?app=` selector) the 版本 tab SHALL render a table from `GET /api/v1/apps/:slug/releases` showing version, Android version code (when present), status (draft / published / yanked), published-at, and created-at.

#### Scenario: Releases render with status

- **GIVEN** the app in scope has a draft and a published release
- **WHEN** the 版本 tab table renders
- **THEN** each row shows its version and a status indicator matching draft / published

### Requirement: Admin SHALL create a draft release

The 版本 tab SHALL provide a create form (version, optional Android version code, optional release notes) that POSTs `/api/v1/apps/:slug/releases`, gated on `release:create`. A duplicate version (`409` conflict) SHALL surface a "version already exists" message. On success the list SHALL refresh and the new release SHALL appear as draft.

#### Scenario: Creating a release adds a draft row

- **GIVEN** an authenticated user holding `release:create`
- **WHEN** the user submits the create form with version `1.2.0`
- **THEN** the list refreshes and shows `1.2.0` with status draft

#### Scenario: Duplicate version is reported

- **GIVEN** version `1.2.0` already exists for the app
- **WHEN** the user submits the create form with version `1.2.0`
- **THEN** a "version already exists" message is shown and no row is added

### Requirement: Admin SHALL publish and yank releases with state-aware affordances

The 版本 tab SHALL offer a publish action only on draft rows (gated on `release:publish`) calling `POST .../publish`, and a yank action only on published rows (gated on `release:yank`) calling `POST .../yank`. Affordances SHALL be hidden when the row's state or the user's permissions disallow the action; the server `409` for an illegal transition SHALL be surfaced as a message if it still occurs.

#### Scenario: Publishing a draft moves it to published

- **GIVEN** a draft release and a user holding `release:publish`
- **WHEN** the user confirms publish
- **THEN** the release becomes published and shows a published-at time

#### Scenario: Yank is not offered on a draft

- **GIVEN** a draft release
- **WHEN** the row's actions render
- **THEN** no yank action is shown for that row

#### Scenario: User without publish permission sees no publish action

- **GIVEN** a draft release and a user lacking `release:publish`
- **WHEN** the row's actions render
- **THEN** no publish action is shown

### Requirement: Admin SHALL view a release's artifacts read-only

The release's artifacts SHALL be presented on the **release detail page**（`/apps/:slug/releases/:version`, no longer in a drawer）as a flat ProTable（one row per artifact）backed by `GET /api/v1/apps/:slug/releases/:version/artifacts`, with columns: platform（merged via `rowSpan` over consecutive same-platform rows）, architecture（friendly label from target triple）, filename, size（right-aligned）, sha256（truncated + copy）, and signature state（status Tag）. An `expandable` row SHALL reveal full sha256, signature, and upload time. Upload is offered via a separate Modal on this page（not inline in the table).

#### Scenario: Artifacts table renders on the detail page

- **GIVEN** a release with a `tauri-desktop` and a `react-native-android` artifact
- **WHEN** the user is on `/apps/:slug/releases/:version`
- **THEN** a ProTable shows each artifact's friendly architecture, filename, size, copyable sha256, and signature Tag
- **AND** consecutive same-platform rows share one merged platform cell

### Requirement: Admin SHALL manage channel release pointers (promote / rollback)

For the app in scope the 渠道 tab SHALL show, per channel, the release the channel currently points at (`GET .../channels/:name/release`, which may be empty). It SHALL allow promoting a published release to a channel (`POST .../channels/:name/promote`, gated on `release:promote`; candidate versions limited to published releases) and rolling back (`POST .../channels/:name/rollback`, gated on `release:rollback`). A rollback with no prior history SHALL surface the `nothing-to-rollback` message. This view is colocated with channel configuration (list / create / set-default) in the same 渠道 tab.

#### Scenario: Promoting points the channel at the release

- **GIVEN** a published release `1.2.0` and a user holding `release:promote`
- **WHEN** the user promotes `1.2.0` to the `beta` channel
- **THEN** the `beta` channel pointer shows `1.2.0` after refetch

#### Scenario: Rollback with no history is reported

- **GIVEN** a channel that has never been promoted and a user holding `release:rollback`
- **WHEN** the user attempts rollback
- **THEN** a "nothing to rollback" message is shown

### Requirement: Admin SHALL open a release detail page

A release row's enter / 「产物」 action SHALL navigate to a **release detail page** at `/apps/:slug/releases/:version`（rendered inside the version tab's outlet）. The page SHALL show the release's metadata（version, status Tag, published time, release notes）with header actions（上传产物 / 编辑 / 发布 / 撤回, permission-gated）, its artifacts table as the body, and a breadcrumb 「应用 / <slug> / 版本 / <version>」. The page SHALL be deep-linkable.

#### Scenario: Entering a release opens its detail page

- **WHEN** the user activates a release row's enter / 「产物」 action for version `0.4.0`
- **THEN** the app navigates to `/apps/swarmnote/releases/0.4.0`
- **AND** the page shows the release metadata, header actions, and the artifacts table

#### Scenario: Release detail deep link resolves

- **WHEN** the user opens `/apps/swarmnote/releases/0.4.0` directly
- **THEN** the release detail page renders without first visiting the version list

### Requirement: Edit release rollout and force-update policy
The release edit drawer SHALL let an operator view and set a release's gray-rollout percentage and force-update floors (Tauri semver `min_version` and RN Android `android_min_version_code`), reusing the existing `PATCH /api/v1/apps/:slug/releases/:version` endpoint, so gray release and force update are configurable from the Admin UI rather than only via CLI/API.

#### Scenario: Set a gray rollout percentage
- **WHEN** an operator edits a release and sets the rollout percentage to 50
- **THEN** the value is persisted and the release subsequently serves to roughly half of clients per the update-check rollout bucketing

#### Scenario: Set a Tauri force-update floor
- **WHEN** an operator edits a release and sets `min_version` to a semver value
- **THEN** the value is persisted and clients below it are force-updated by the Tauri update-check endpoint

#### Scenario: Clearing a set floor removes it
- **WHEN** an operator clears the `min_version` field of a release that currently has a force-update floor and saves
- **THEN** the floor is removed (the client maps the now-empty field to the `0.0.0` sentinel)

#### Scenario: Leaving an unset field empty changes nothing
- **WHEN** an operator saves the edit drawer with `min_version` empty on a release that had no floor
- **THEN** the stored value is unchanged (the client sends no-change rather than drifting NULL to a sentinel)

#### Scenario: Detail page shows the current policy
- **WHEN** an operator views a release detail page
- **THEN** the current rollout percentage and force-update floors are displayed
