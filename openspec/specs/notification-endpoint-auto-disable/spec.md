# notification-endpoint-auto-disable Specification

## Purpose
TBD - created by archiving change add-notification-endpoint-auto-disable. Update Purpose after archive.
## Requirements
### Requirement: Webhook endpoint failure health tracking

The worker SHALL track a webhook endpoint's failure health: a successful delivery clears the failing-since marker, and a dead delivery sets it when first unset, so the endpoint records when its current failing streak began.

#### Scenario: A successful delivery clears the failing marker

- **WHEN** a webhook delivery to an endpoint reaches the sent state
- **THEN** the endpoint's failing-since marker is cleared

#### Scenario: A dead delivery records when failing began

- **WHEN** a webhook delivery reaches the dead state and the endpoint has no failing-since marker
- **THEN** the endpoint's failing-since marker is set to the current time

### Requirement: Auto-disable on sustained failure

The worker SHALL auto-disable a webhook endpoint once it has been failing for longer than the auto-disable threshold, and the endpoint view SHALL expose the failing-since marker so operators can see why an endpoint is disabled.

#### Scenario: Endpoint auto-disables after the threshold

- **WHEN** a webhook delivery reaches the dead state and the endpoint has been failing for longer than the threshold
- **THEN** the endpoint is set to disabled
- **AND** the failing-since marker is retained as the reason marker

#### Scenario: Re-enabling clears the failing marker

- **WHEN** an operator updates a disabled endpoint to enabled
- **THEN** the endpoint's failing-since marker is cleared

#### Scenario: View exposes the failing marker without the secret

- **WHEN** an operator lists webhook endpoints
- **THEN** each endpoint view includes its failing-since marker
- **AND** no endpoint response includes any plaintext secret
