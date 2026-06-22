# notification-delivery-log Specification

## Purpose
TBD - created by archiving change add-notification-delivery-payload-log. Update Purpose after archive.
## Requirements
### Requirement: Delivery request/response snapshot

The notification worker SHALL persist, on each webhook delivery attempt, the request body, the webhook-timestamp and webhook-signature headers actually sent, and the response body (truncated), overwriting the previous attempt's snapshot.

#### Scenario: Successful webhook delivery captures the response body

- **WHEN** a webhook delivery succeeds with a 2xx response
- **THEN** the delivery row stores the request body, the sent webhook-timestamp and webhook-signature, and the response body

#### Scenario: Failed webhook delivery captures the snapshot too

- **WHEN** a webhook delivery returns a non-2xx response
- **THEN** the delivery row stores the same request snapshot and the error response body

#### Scenario: Email delivery has no HTTP snapshot

- **WHEN** an email delivery is performed
- **THEN** the request/response snapshot fields remain null

#### Scenario: Oversized response body is truncated

- **WHEN** a webhook response body exceeds the size cap
- **THEN** the stored response body is truncated to the cap

### Requirement: Delivery detail endpoint

The server SHALL expose `GET /api/v1/notifications/deliveries/{id}` returning the delivery together with its request/response snapshot, gated by the `notification:manage` permission.

#### Scenario: Detail returns the snapshot

- **WHEN** an authorized caller requests a delivery by id
- **THEN** the response includes the delivery fields plus the request body, request timestamp, request signature, and response body

#### Scenario: Unknown id is not found

- **WHEN** the delivery id does not exist
- **THEN** the endpoint returns 404

#### Scenario: Unauthorized caller rejected

- **WHEN** a caller without `notification:manage` requests a delivery detail
- **THEN** the endpoint returns 403

