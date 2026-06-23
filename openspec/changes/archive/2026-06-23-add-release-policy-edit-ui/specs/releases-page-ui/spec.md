## ADDED Requirements

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
