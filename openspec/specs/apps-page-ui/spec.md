# apps-page-ui Specification

## Purpose
TBD - created by archiving change add-apps-page-ui. Update Purpose after archive.
## Requirements
### Requirement: Admin SHALL list applications in a table

The `/apps` page SHALL render a `ProTable` whose rows come from `GET /api/v1/apps`, showing display name, slug, platforms, and created-at. (The default channel is not a list column — the `App` resource does not carry it; it is shown and managed in the per-app channels view.) The page SHALL be reachable only inside the `_auth` guard (authenticated). When the API returns an empty list the table SHALL render an empty state, not an error.

#### Scenario: Authenticated user sees existing apps

- **GIVEN** an authenticated session and two apps exist
- **WHEN** the user navigates to `/apps`
- **THEN** the table renders two rows with each app's slug and platforms
- **AND** no error notification is shown

#### Scenario: Empty list renders empty state

- **GIVEN** an authenticated session and zero apps
- **WHEN** the user navigates to `/apps`
- **THEN** the table renders an empty state and no error

### Requirement: Admin SHALL create an application

The page SHALL provide a create form (slug, display name, platforms multi-select) that POSTs `/api/v1/apps`. On success the apps list SHALL be invalidated so the new row appears. A duplicate slug response (`409`, RFC 9457 `type` for conflict) SHALL surface an inline "slug already exists" message rather than a generic error. The create affordance SHALL be gated on the `app:create` permission.

#### Scenario: Owner creates an app

- **GIVEN** an Owner session on `/apps`
- **WHEN** the Owner submits the create form with slug `swarmdrop`, a display name, and platform `tauri-desktop`
- **THEN** the request POSTs `/api/v1/apps`
- **AND** on success the table refetches and shows a row with slug `swarmdrop`

#### Scenario: Duplicate slug is reported inline

- **GIVEN** an app with slug `swarmdrop` already exists
- **WHEN** the user submits the create form with slug `swarmdrop`
- **THEN** the form shows a "slug already exists" message
- **AND** no new row is added

#### Scenario: User without app:create cannot create

- **GIVEN** an authenticated user whose permissions do not include `app:create`
- **WHEN** the user views `/apps`
- **THEN** the create button is not rendered

### Requirement: Admin SHALL edit an application

The page SHALL provide an edit form that PATCHes `/api/v1/apps/:slug` with display name and platforms. (The default channel is set in the channels view, not this form, to avoid two redundant controls for the same state.) The form SHALL pre-fill the selected row's current values on each open (re-mounting per row so stale values from a prior edit never leak). `slug` SHALL NOT be editable. The edit affordance SHALL be gated on `app:update`.

#### Scenario: Editing two apps in sequence shows correct values

- **GIVEN** apps `alpha` and `beta` exist
- **WHEN** the user opens edit for `alpha`, closes it, then opens edit for `beta`
- **THEN** the form shows `beta`'s current values, not `alpha`'s

#### Scenario: slug is not editable

- **WHEN** the user opens the edit form for an app
- **THEN** there is no editable slug field

### Requirement: Admin SHALL delete an application and surface the has-releases block

The page SHALL provide a delete action (confirmation prompt) that DELETEs `/api/v1/apps/:slug`, gated on `app:delete`. When the server responds `409` with the app-has-releases `type`, the UI SHALL show a message stating the app still has releases and SHALL leave the row in place.

#### Scenario: Deleting an app with releases is blocked with a clear message

- **GIVEN** an app `swarmdrop` that has at least one release
- **WHEN** the user confirms delete on `swarmdrop`
- **THEN** a notification states the app still has releases and cannot be deleted
- **AND** the `swarmdrop` row remains in the table

#### Scenario: Deleting an empty app succeeds

- **GIVEN** an app `scratch` with zero releases
- **WHEN** the user confirms delete on `scratch`
- **THEN** the request succeeds and the row disappears after refetch

### Requirement: Admin SHALL manage an application's channels

From a row action the page SHALL open a channels view backed by `GET /api/v1/apps/:slug/channels` that lists each channel and which is default. The view SHALL allow creating a custom channel (`POST /api/v1/apps/:slug/channels`) and setting a channel default (`PATCH /api/v1/apps/:slug/channels/:name` with `is_default: true`); setting a new default SHALL reflect the previous default being unset after refetch. These affordances SHALL be gated on `app:update`. No channel deletion is offered (the server provides no channel DELETE).

#### Scenario: Set a different channel as default

- **GIVEN** an app whose default channel is `stable`
- **WHEN** the user sets `beta` as default in the channels view
- **THEN** after refetch `beta` is shown default and `stable` is not

#### Scenario: Create a custom channel

- **GIVEN** the channels view for an app
- **WHEN** the user adds a custom channel `nightly`
- **THEN** after refetch `nightly` appears in the channel list

