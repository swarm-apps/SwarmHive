# notifications-cli Specification

## Purpose
TBD - created by archiving change add-notifications-cli. Update Purpose after archive.
## Requirements
### Requirement: Notification webhook endpoint CLI

The CLI SHALL provide `swarmhive notifications endpoints` subcommands to list, create, update, delete, rotate the secret of, and test webhook endpoints, printing the `whsec_` signing secret exactly once on create and rotate.

#### Scenario: Create prints the signing secret once

- **WHEN** a user runs `swarmhive notifications endpoints create --name N --url U`
- **THEN** the endpoint is created and the `whsec_` signing secret is printed once
- **AND** with `--output json` the full create response (including the secret) is emitted as a JSON object

#### Scenario: Update toggles enabled state

- **WHEN** a user runs `swarmhive notifications endpoints update --endpoint <id|name> --disable`
- **THEN** the endpoint is patched with disabled=true

#### Scenario: Delete requires confirmation

- **WHEN** a user runs `swarmhive notifications endpoints delete --endpoint <id|name>` without `--yes`
- **THEN** the command refuses and exits non-zero

#### Scenario: Test does not write a delivery log entry

- **WHEN** a user runs `swarmhive notifications endpoints test --endpoint <id|name>`
- **THEN** a signed webhook.test request is attempted and its result is printed without creating a delivery record

### Requirement: Notification subscription CLI

The CLI SHALL provide `swarmhive notifications subscriptions` subcommands to list, create, and delete subscriptions that bind an event type to an email address or webhook endpoint, optionally scoped to a single app.

#### Scenario: Create an email subscription

- **WHEN** a user runs `swarmhive notifications subscriptions create --event release.published --channel email --to a@b.c`
- **THEN** a subscription bound to that email and event type is created

#### Scenario: Create a webhook subscription scoped to an app

- **WHEN** a user runs `swarmhive notifications subscriptions create --event channel.promoted --channel webhook --endpoint <id|name> --app <slug>`
- **THEN** the app slug is resolved to its id and a subscription bound to that endpoint and app is created

#### Scenario: Delete requires confirmation

- **WHEN** a user runs `swarmhive notifications subscriptions delete --id <uuid>` without `--yes`
- **THEN** the command refuses and exits non-zero

### Requirement: Notification delivery CLI

The CLI SHALL provide `swarmhive notifications deliveries` subcommands to list deliveries filtered by endpoint and status and to redeliver a delivery preserving its original webhook-id.

#### Scenario: List filtered by endpoint and status

- **WHEN** a user runs `swarmhive notifications deliveries list --endpoint <id|name> --status failed`
- **THEN** only failed deliveries for that endpoint are listed

#### Scenario: Redeliver re-enqueues preserving identity

- **WHEN** a user runs `swarmhive notifications deliveries redeliver --id <uuid>`
- **THEN** the delivery is re-enqueued preserving its original webhook-id

