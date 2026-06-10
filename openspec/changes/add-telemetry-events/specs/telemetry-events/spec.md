# telemetry-events

## ADDED Requirements

### Requirement: Server SHALL persist native update-link events with a result dimension

The server SHALL record one `update_event` row per update-check and per download-intent,
written server-side (trusted source). `update_check` rows SHALL carry
`result ∈ {up_to_date, available, rollout_held}` (the `update_available` concept is absorbed
into `result=available`); `download_intent` rows SHALL carry `result ∈ {redirected, failed}`.
Rows SHALL include app/release/channel/version/platform/target/arch/abi/artifact dimensions
and the optional `client_id`, and SHALL NOT include raw IP or User-Agent columns.
Event persistence failures SHALL be swallowed (logged) and SHALL NOT affect the
update-check or download response.

#### Scenario: Update check with rollout miss is recorded as rollout_held

- **GIVEN** a release with rollout < 100 and a client whose bucket is outside the rollout
- **WHEN** the client calls the update-check endpoint
- **THEN** the endpoint responds exactly as before (no behavioural change)
- **AND** an `update_event` row exists with `event_name='update_check'`, `result='rollout_held'`,
  the client's `current_version`, and its `client_id`

#### Scenario: Event write failure does not break the check

- **GIVEN** the telemetry insert fails (e.g. table dropped in test)
- **WHEN** a client calls the update-check endpoint
- **THEN** the response is still the normal 200/204
- **AND** a warning is logged

### Requirement: Server SHALL accept SDK-reported client events on a public rate-limited endpoint

The server SHALL expose `POST /api/v1/events` (public, governor rate-limited, single event
per request) accepting exactly six event names: `download_started`, `download_completed`,
`download_failed`, `install_started`, `install_failed`, `app_started_after_update`.
The body SHALL require `event`, `app` (slug), `platform`, `client_id` (≤64 chars), and MAY
include `channel`, `target_version`, `previous_version`, `bytes_total`, `duration_ms`,
`error_code`, `error_message` (server-side truncated to 512). Unknown app SHALL return 404;
unknown event names SHALL fail validation; persistence failures SHALL still return 200.
Rows are stored in `client_event`, physically separate from trusted `update_event` rows.

#### Scenario: Download completion is reported and stored

- **GIVEN** an existing app `swarmnote-rn`
- **WHEN** a client POSTs `/api/v1/events` with
  `{ event: "download_completed", app: "swarmnote-rn", platform: "android", client_id: "...", target_version: "0.3.0", bytes_total: 52428800 }`
- **THEN** the response is `200`
- **AND** a `client_event` row exists with those fields

#### Scenario: Unknown app and oversized error message are handled

- **GIVEN** no app with slug `ghost`
- **WHEN** a client POSTs an event for `app: "ghost"`
- **THEN** the response is `404`
- **WHEN** a client POSTs a valid event whose `error_message` exceeds 512 chars
- **THEN** the stored row's `error_message` is truncated to 512

### Requirement: Rollup SHALL separate additive counts from non-additive device uniques

A periodic server task (hourly) SHALL recompute the day buckets for today and yesterday
(idempotent delete+insert) into two tables: `event_rollup_day` with fully-expanded dimensions
`(app_id, day, source, event_name, result, version, platform, channel) → count` (additive),
and `device_rollup_day` with `(app_id, day, version) → unique_clients` computed as
`COUNT(DISTINCT client_id)` over `update_check` events only (a `version=NULL` row holds the
app's total daily active devices). Re-running the rollup SHALL NOT change results.

#### Scenario: Per-version unique devices match raw distinct counts

- **GIVEN** raw `update_event` rows from 3 distinct clients on version 1.0.0 and 2 on 1.1.0
  (one client checking twice)
- **WHEN** the rollup task runs
- **THEN** `device_rollup_day` has `unique_clients=3` for 1.0.0 and `2` for 1.1.0
- **AND** the `version=NULL` row has `unique_clients=5`
- **AND** running the task again leaves identical rows

### Requirement: Raw events SHALL be retained short-term while rollups persist forever

A daily server task SHALL delete `update_event` / `client_event` rows older than
`telemetry.raw_retention_days` (default 90; `0` disables cleanup). Rollup tables SHALL never
be cleaned. Adoption metrics SHALL remain queryable after raw cleanup.

#### Scenario: Cleanup removes old raw rows but keeps aggregates

- **GIVEN** raw events older than 90 days (forged `created_at`) whose day buckets were rolled up
- **WHEN** the cleanup task runs
- **THEN** those raw rows are deleted
- **AND** the corresponding `device_rollup_day` / `event_rollup_day` rows still exist

### Requirement: Admin SHALL expose telemetry query endpoints gated on telemetry:read

The server SHALL expose `GET /api/v1/telemetry/{summary,adoption,funnel,distribution}`
(query params `app`, `days`, distribution also `dim`), all reading rollup tables only and
gated on the existing `telemetry:read` permission. The funnel SHALL count occurrences
(not unique devices) across `update_check(result=available)` → `download_intent(result=redirected)`
→ `download_completed` → `app_started_after_update`.

#### Scenario: Viewer can read, anonymous cannot

- **GIVEN** a `viewer` session (role has `telemetry:read`)
- **WHEN** it GETs `/api/v1/telemetry/adoption?app=swarmdrop&days=30`
- **THEN** the response is `200` with per-version daily unique-device series
- **WHEN** an anonymous client GETs the same endpoint
- **THEN** the response is `401`

### Requirement: Admin SPA SHALL render a top-level telemetry page

The Admin SPA SHALL add a top-level「统计」menu item (visible only with `telemetry:read`)
routing to `/telemetry`, containing: app selector + day-range selector (7/30/90), metric cards
(today's active devices, downloads completed, latest-version Active%), an adoption chart
(unique devices by version over time), the update funnel with conversion percentages
(occurrence-based, labelled as such), platform/arch distribution, and a version long-tail table.
The disabled「遥测」placeholder under Settings SHALL be removed. Charts use `@ant-design/plots`.

#### Scenario: Telemetry page renders adoption and funnel

- **GIVEN** rollup data exists for app `swarmdrop`
- **WHEN** a user with `telemetry:read` opens `/telemetry` and selects the app
- **THEN** the adoption chart shows per-version device series
- **AND** the funnel shows counts and conversion percentages for its stages

#### Scenario: No permission hides the page

- **GIVEN** a user whose roles lack `telemetry:read`
- **WHEN** they view the sidebar
- **THEN** the「统计」item is absent
- **AND** direct API calls to `/api/v1/telemetry/*` return `403`

### Requirement: Privacy defaults SHALL hold across the pipeline

No table SHALL contain raw IP or raw User-Agent. `client_id` SHALL be treated as a
pseudonymous identifier: documentation SHALL state the operator is the data controller and
SHALL document the SDK contract (silent-failure reporting, `telemetry: false` opt-out hook
for app developers, client_id reset capability). Geo lookup SHALL NOT be performed.

#### Scenario: Schema contains no IP or UA columns

- **GIVEN** the synced schema
- **WHEN** inspecting `update_event` and `client_event` columns
- **THEN** no `ip` or `user_agent` column exists
