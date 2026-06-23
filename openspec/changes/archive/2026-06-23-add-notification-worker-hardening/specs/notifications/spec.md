## ADDED Requirements

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
