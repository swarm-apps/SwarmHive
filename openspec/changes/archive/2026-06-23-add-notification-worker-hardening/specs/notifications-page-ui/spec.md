## ADDED Requirements

### Requirement: Rotate-secret affordance only for generic endpoints
The webhook endpoint list SHALL show the "rotate secret" action only for generic (Standard Webhooks) endpoints, because the server rejects rotation for IM providers whose signing secret is user-owned, so the action MUST NOT be offered where it would always fail.

#### Scenario: Generic endpoint offers rotation
- **WHEN** an operator views a generic webhook endpoint row
- **THEN** the "rotate secret" action is shown

#### Scenario: IM provider endpoint hides rotation
- **WHEN** an operator views a feishu, slack, dingtalk, or discord endpoint row
- **THEN** the "rotate secret" action is not shown
