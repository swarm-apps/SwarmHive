# notifications Specification

## Purpose
TBD - created by archiving change add-notifications. Update Purpose after archive.
## Requirements
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

#### Scenario: Update webhook endpoint metadata
- **WHEN** an operator updates a webhook endpoint name, URL, or disabled flag
- **THEN** the server validates the new URL, persists the new metadata, and continues hiding the signing secret

#### Scenario: Test webhook endpoint
- **WHEN** an operator triggers a webhook endpoint test
- **THEN** the server sends one signed `webhook.test` request to that endpoint without creating a delivery log entry

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

### Requirement: Delivery executes outside the database transaction
The delivery worker SHALL perform each external delivery (webhook POST or email send) outside of any open database transaction, and SHALL persist each delivery's result in its own short transaction, so that a slow or failing delivery neither holds row locks during the external call nor forces other deliveries in the same batch to be re-sent.

#### Scenario: Row locks are released before the external call
- **WHEN** the worker dispatches a batch of due deliveries
- **THEN** it claims the due rows under `FOR UPDATE SKIP LOCKED` and commits that claim transaction before performing any external HTTP or SMTP call

#### Scenario: One failed persist does not re-send its siblings
- **WHEN** one delivery in a batch fails to persist its result after the external call already happened
- **THEN** the other deliveries in the same batch keep their committed results and are not re-sent on the next tick

### Requirement: Indexed notification polling tables
The notification outbox, delivery, subscription, and delivery-attempt tables SHALL carry indexes covering the worker's polling and listing predicates, so periodic dispatch does not degrade as these append-only tables grow.

#### Scenario: Due-delivery scan is index-backed
- **WHEN** the worker scans for due deliveries by status and next-retry time
- **THEN** the query is served by an index on `(status, next_retry_at, created_at)` rather than a sequential scan

#### Scenario: Indexes are created idempotently on every boot
- **WHEN** the server starts and runs its data migrations more than once
- **THEN** the notification indexes are created if absent and left unchanged if already present, without error
