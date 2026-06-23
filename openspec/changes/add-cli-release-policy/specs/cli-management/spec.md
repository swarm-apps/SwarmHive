## ADDED Requirements

### Requirement: CLI SHALL set release rollout and force-update policy
The `swarmhive releases update` command SHALL expose flags to set a release's gray-rollout percentage and force-update floors (`--rollout-percent`, `--min-version`, `--android-min-version-code`), and `swarmhive releases create` SHALL expose `--android-min-version-code`, mapping each provided flag to the corresponding `UpdateReleaseRequest`/`CreateReleaseRequest` field so gray release and force update are configurable from CI/CLI, at parity with the Admin UI.

#### Scenario: Set rollout and floor via CLI
- **WHEN** an operator runs `swarmhive releases update --app a --version 1.2.0 --rollout-percent 50 --min-version 1.0.0`
- **THEN** the request sets `rollout_percent=50` and `min_version=1.0.0` on that release

#### Scenario: Omitted policy flags leave values unchanged
- **WHEN** an operator runs `releases update` without any policy flag
- **THEN** the policy fields are sent as absent (no change), preserving the stored values

#### Scenario: Clearing uses explicit sentinels
- **WHEN** an operator passes `--rollout-percent 100` or `--min-version 0.0.0`
- **THEN** gray rollout is disabled (full) or the force-update floor is removed, matching the server's single-Option sentinel semantics
