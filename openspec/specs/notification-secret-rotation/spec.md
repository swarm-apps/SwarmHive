# notification-secret-rotation Specification

## Purpose
TBD - created by archiving change add-notification-secret-rotation-grace. Update Purpose after archive.
## Requirements
### Requirement: Webhook secret rotation grace window

Rotating a webhook endpoint's signing secret SHALL retain the previous secret for a 24-hour grace window during which deliveries are signed with both the new and the previous secret, so receivers can switch over without downtime.

#### Scenario: Rotation keeps the previous secret valid

- **WHEN** an operator rotates a webhook endpoint secret
- **THEN** the endpoint stores the previous secret with an expiry 24 hours in the future
- **AND** a brand-new plaintext secret is returned exactly once

#### Scenario: Deliveries dual-sign during the grace window

- **WHEN** a webhook delivery is sent while the previous secret has not expired
- **THEN** the `webhook-signature` header contains two space-separated `v1,` signatures
- **AND** both the new and the previous secret verify their respective signature

#### Scenario: Only the current secret signs after expiry

- **WHEN** a webhook delivery is sent after the previous secret's expiry
- **THEN** the `webhook-signature` header contains a single `v1,` signature for the current secret

#### Scenario: Endpoint view exposes the grace expiry but not the secret

- **WHEN** an operator lists webhook endpoints during a grace window
- **THEN** the endpoint view includes the previous secret expiry timestamp
- **AND** no endpoint response includes any plaintext secret

