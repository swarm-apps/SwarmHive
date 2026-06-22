## ADDED Requirements

### Requirement: Transactional event emission
The server SHALL emit notification events (`release.published`, `channel.promoted`, `channel.rolled_back`) into a Postgres outbox within the same database transaction as the originating business change, so a rolled-back operation produces no notification and a committed one is never lost.

#### Scenario: Publish emits within the same transaction
- **WHEN** a release is published successfully
- **THEN** a `release.published` outbox row is committed atomically with the release state change

#### Scenario: Rolled-back operation emits nothing
- **WHEN** a publish transaction fails and rolls back
- **THEN** no notification outbox row persists for that attempt

### Requirement: Event subscriptions
An operator with `notification:manage` SHALL be able to create, list, and delete subscriptions that bind a channel to one or more event types, optionally scoped to a specific app.

#### Scenario: Subscribe a channel to release events
- **WHEN** an operator subscribes a webhook channel to `release.published` scoped to app `swarmdrop`
- **THEN** subsequent `release.published` events for `swarmdrop` are routed to that channel

#### Scenario: Unscoped subscription receives all apps
- **WHEN** a subscription has no app scope
- **THEN** matching events from every app are routed to it

### Requirement: Email notification channel
The email channel SHALL deliver notifications through the existing `Mailer`, reusing its active provider configuration and templating rather than introducing a second mail path.

#### Scenario: Email delivery on release
- **WHEN** a `release.published` event matches an email subscription
- **THEN** the server sends an email via the active mail provider carrying the release version, app, and notes

### Requirement: Standard Webhooks signing
The outgoing webhook channel SHALL sign each delivery per the Standard Webhooks specification, sending a `webhook-id` (unique, stable across redeliveries), a `webhook-timestamp` (unix seconds), and a `webhook-signature` header of form `v1,<base64>` whose signature is HMAC-SHA256 over the exact string `{webhook-id}.{webhook-timestamp}.{raw-body}`.

#### Scenario: Signed webhook delivery
- **WHEN** the server posts a webhook for an event
- **THEN** the request carries `webhook-id`, `webhook-timestamp`, and a `webhook-signature` of `v1,<base64 HMAC-SHA256>` computed over `id.timestamp.rawbody`

#### Scenario: Redelivery reuses the webhook-id
- **WHEN** a failed delivery is retried or manually re-sent
- **THEN** the `webhook-id` is unchanged so a receiver can deduplicate by it

### Requirement: Webhook endpoint secret management
A webhook endpoint signing secret SHALL be stored encrypted at rest (AES-256-GCM, reusing `crypto::SecretKey`) and SHALL be returned in plaintext exactly once, at creation time.

#### Scenario: Secret revealed once
- **WHEN** a webhook endpoint is created
- **THEN** the response includes the `whsec_`-prefixed plaintext secret exactly once, and subsequent reads never expose it

### Requirement: Reliable at-least-once delivery
Notification delivery SHALL be at-least-once with bounded exponential-backoff retries, and a delivery that exhausts its maximum retry budget SHALL be marked dead rather than retried indefinitely.

#### Scenario: Retry on transient failure
- **WHEN** a webhook POST returns a 5xx status or times out
- **THEN** the delivery is re-scheduled with exponential backoff until it succeeds or reaches the maximum attempts

#### Scenario: Dead-letter after max attempts
- **WHEN** a delivery exhausts its retry budget
- **THEN** it is marked `dead` and is no longer auto-retried

### Requirement: Delivery log and manual redelivery
The server SHALL record every delivery attempt (status, response code, attempt count, timestamp) and SHALL expose an endpoint to manually re-enqueue a recorded delivery.

#### Scenario: List deliveries
- **WHEN** an operator lists deliveries for a webhook endpoint
- **THEN** each attempt's status, response code, and attempt count are returned

#### Scenario: Manual redelivery
- **WHEN** an operator triggers redelivery of a delivery id
- **THEN** the server re-enqueues it for sending, preserving the original `webhook-id`

### Requirement: RBAC gating
All notification management endpoints SHALL require the `notification:manage` permission and SHALL reject callers lacking it.

#### Scenario: Unauthorized caller rejected
- **WHEN** a caller without `notification:manage` calls a notification management endpoint
- **THEN** the server responds 403 with an RFC 9457 problem document
