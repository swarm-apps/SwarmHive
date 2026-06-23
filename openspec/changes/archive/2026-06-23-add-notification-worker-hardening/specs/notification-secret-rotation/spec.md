## ADDED Requirements

### Requirement: Rotation is rejected during an active grace window
Rotating a webhook endpoint secret SHALL be rejected while a previous secret is still within its unexpired grace window, because the single previous-secret slot can hold only one prior key and a second rotation would overwrite it, breaking receivers that still use the earliest secret.

#### Scenario: Second rotation within the grace window is rejected
- **WHEN** an operator rotates a webhook endpoint secret and then rotates it again before the previous secret expires
- **THEN** the second rotation is rejected with a 409 conflict and the stored current and previous secrets are unchanged

#### Scenario: Rotation is allowed again after the grace window expires
- **WHEN** an operator rotates a webhook endpoint secret after the previous secret's grace window has expired
- **THEN** the rotation succeeds and a brand-new plaintext secret is returned exactly once
