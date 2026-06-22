## ADDED Requirements

### Requirement: Webhook endpoint provider kind

A webhook endpoint SHALL carry a provider kind (generic, feishu, slack, dingtalk, or discord) that selects how deliveries are signed and formatted, defaulting to generic.

#### Scenario: Default provider kind is generic

- **WHEN** an operator creates a webhook endpoint without specifying a provider
- **THEN** the endpoint's provider kind is generic and a `whsec_` signing secret is returned once

#### Scenario: IM provider stores the user-supplied secret

- **WHEN** an operator creates a feishu or dingtalk endpoint with a signing secret
- **THEN** the secret is stored encrypted and the create response carries no SwarmHive-generated secret

#### Scenario: Rotate applies only to generic

- **WHEN** an operator rotates the secret of a non-generic endpoint
- **THEN** the request is rejected as a validation error

### Requirement: Platform-native IM delivery

Deliveries to an IM provider endpoint SHALL render a platform-native message, apply that platform's signing where configured, and determine success by that platform's rule.

#### Scenario: Feishu delivery is a signed interactive card

- **WHEN** a release event is delivered to a feishu endpoint that has a signing secret
- **THEN** the request body is an interactive card carrying a `timestamp` and `sign` computed over the empty payload keyed by `{timestamp}\n{secret}`
- **AND** success is determined by the response body's `code` being zero

#### Scenario: Slack delivery is unsigned Block Kit

- **WHEN** a release event is delivered to a slack endpoint
- **THEN** the request body carries Block Kit `blocks` with no signature headers
- **AND** success requires an HTTP 200 with an `ok` body

#### Scenario: Dingtalk signs the URL query

- **WHEN** a release event is delivered to a dingtalk endpoint that has a signing secret
- **THEN** the request URL carries a `timestamp` and url-encoded `sign`, and success is determined by the response body's `errcode` being zero

#### Scenario: Discord delivery is an unsigned embed

- **WHEN** a release event is delivered to a discord endpoint
- **THEN** the request body carries an `embeds` array with no signature, and success requires an HTTP 2xx response
