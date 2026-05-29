# releases-page-ui Specification

## Purpose
TBD - created by archiving change add-releases-page-ui. Update Purpose after archive.
## Requirements
### Requirement: Admin SHALL select an app before listing releases

Because releases are addressed under `/api/v1/apps/:slug/...` with no cross-app list endpoint, the `/releases` page SHALL provide an app selector sourced from `GET /api/v1/apps`, and SHALL persist the selected slug in the URL search parameter `?app=<slug>` (validated). When no app is selected the page SHALL prompt selection; when no apps exist it SHALL show an empty state linking to `/apps`. Release queries SHALL be disabled until a slug is selected.

#### Scenario: Selecting an app lists its releases and updates the URL

- **GIVEN** at least one app exists and the user is on `/releases`
- **WHEN** the user selects app `swarmdrop`
- **THEN** the URL includes `?app=swarmdrop`
- **AND** the table lists `swarmdrop`'s releases

#### Scenario: Reloading preserves the selected app

- **GIVEN** the user is on `/releases?app=swarmdrop`
- **WHEN** the page reloads
- **THEN** `swarmdrop` remains selected and its releases are listed

#### Scenario: No apps shows guidance

- **GIVEN** zero apps exist
- **WHEN** the user opens `/releases`
- **THEN** an empty state directs the user to create an app at `/apps`

### Requirement: Admin SHALL list releases with lifecycle state

For the selected app the page SHALL render a table from `GET /api/v1/apps/:slug/releases` showing version, Android version code (when present), status (draft / published / yanked), published-at, and created-at.

#### Scenario: Releases render with status

- **GIVEN** the selected app has a draft and a published release
- **WHEN** the table renders
- **THEN** each row shows its version and a status indicator matching draft / published

### Requirement: Admin SHALL create a draft release

The page SHALL provide a create form (version, optional Android version code, optional release notes) that POSTs `/api/v1/apps/:slug/releases`, gated on `release:create`. A duplicate version (`409` conflict) SHALL surface a "version already exists" message. On success the list SHALL refresh and the new release SHALL appear as draft.

#### Scenario: Creating a release adds a draft row

- **GIVEN** an authenticated user holding `release:create`
- **WHEN** the user submits the create form with version `1.2.0`
- **THEN** the list refreshes and shows `1.2.0` with status draft

#### Scenario: Duplicate version is reported

- **GIVEN** version `1.2.0` already exists for the app
- **WHEN** the user submits the create form with version `1.2.0`
- **THEN** a "version already exists" message is shown and no row is added

### Requirement: Admin SHALL publish and yank releases with state-aware affordances

The page SHALL offer a publish action only on draft rows (gated on `release:publish`) calling `POST .../publish`, and a yank action only on published rows (gated on `release:yank`) calling `POST .../yank`. Affordances SHALL be hidden when the row's state or the user's permissions disallow the action; the server `409` for an illegal transition SHALL be surfaced as a message if it still occurs.

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

From a row action the page SHALL open a view backed by `GET /api/v1/apps/:slug/releases/:version/artifacts` listing each artifact's platform, target/arch/abi (as present), filename, size, and sha256. No upload or delete is offered here.

#### Scenario: Artifacts view lists binaries

- **GIVEN** a release with one artifact
- **WHEN** the user opens its artifacts view
- **THEN** the artifact's filename, size, and sha256 are shown

### Requirement: Admin SHALL manage channel release pointers (promote / rollback)

For the selected app the page SHALL show, per channel, the release the channel currently points at (`GET .../channels/:name/release`, which may be empty). It SHALL allow promoting a published release to a channel (`POST .../channels/:name/promote`, gated on `release:promote`; candidate versions limited to published releases) and rolling back (`POST .../channels/:name/rollback`, gated on `release:rollback`). A rollback with no prior history SHALL surface the `nothing-to-rollback` message.

#### Scenario: Promoting points the channel at the release

- **GIVEN** a published release `1.2.0` and a user holding `release:promote`
- **WHEN** the user promotes `1.2.0` to the `beta` channel
- **THEN** the `beta` channel pointer shows `1.2.0` after refetch

#### Scenario: Rollback with no history is reported

- **GIVEN** a channel that has never been promoted and a user holding `release:rollback`
- **WHEN** the user attempts rollback
- **THEN** a "nothing to rollback" message is shown

