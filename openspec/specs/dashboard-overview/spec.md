# dashboard-overview Specification

## Purpose
TBD - created by archiving change add-dashboard-overview. Update Purpose after archive.
## Requirements
### Requirement: Global telemetry overview endpoint
The server SHALL expose `GET /api/v1/telemetry/overview` gated by `telemetry:read`, returning a cross-app at-a-glance overview — total app count, total release count, in-period update-check and download-completed totals, and a per-day activity trend — aggregating only the additive `event_rollup_day` counts and never summing the non-additive distinct-device rollups.

#### Scenario: Overview returns global counts
- **WHEN** an operator with `telemetry:read` requests the overview after apps and releases exist
- **THEN** the response reports the total app and release counts and the in-period update-check and download-completed totals summed across all apps

#### Scenario: Overview excludes distinct-device sums
- **WHEN** the overview is computed
- **THEN** it derives activity metrics from `event_rollup_day` only and does not sum `device_rollup_day`, because distinct-device counts are not additive across apps

#### Scenario: Overview is permission gated
- **WHEN** a caller without `telemetry:read` requests the overview
- **THEN** the server responds 403 with an RFC 9457 problem document

### Requirement: Home dashboard shows real data
The admin home dashboard SHALL render the live overview — real metric cards and a real activity trend over a selectable window — instead of placeholder zeros, and SHALL degrade gracefully when the signed-in user lacks `telemetry:read`.

#### Scenario: Dashboard renders live metrics
- **WHEN** a user with `telemetry:read` opens the home dashboard
- **THEN** the metric cards and trend chart show values fetched from the overview endpoint, not hardcoded zeros

#### Scenario: Window selection refetches
- **WHEN** the user switches the dashboard window between 7, 30, and 90 days
- **THEN** the overview is refetched for the selected window and the cards and trend update
