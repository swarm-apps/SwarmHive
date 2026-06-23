## ADDED Requirements

### Requirement: CLI SHALL query telemetry
The CLI SHALL provide `swarmhive telemetry {overview,summary,adoption,funnel,distribution}` read-only commands that consume the existing `telemetry:read`-gated query endpoints, so release adoption, the update funnel, and distributions can be inspected from CI/scripts without opening the Web Admin, supporting both table and `--output json`.

#### Scenario: Global overview from the CLI
- **WHEN** an operator runs `swarmhive telemetry overview --days 30`
- **THEN** the CLI prints the cross-app app/release counts and in-period update-check and download totals returned by `GET /api/v1/telemetry/overview`

#### Scenario: Per-app funnel from the CLI
- **WHEN** an operator runs `swarmhive telemetry funnel --app myapp --days 7`
- **THEN** the CLI prints each funnel stage with its count and conversion percentage

#### Scenario: JSON output for scripting
- **WHEN** an operator runs any telemetry command with `--output json`
- **THEN** the response DTO is printed as JSON for downstream tooling

#### Scenario: Permission gated
- **WHEN** a caller whose token lacks `telemetry:read` runs a telemetry command
- **THEN** the server responds 403 and the CLI surfaces the RFC 9457 problem
