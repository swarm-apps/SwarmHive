# notifications-page-ui Specification

## Purpose
TBD - created by archiving change add-notifications-page-ui. Update Purpose after archive.
## Requirements
### Requirement: Notification management page

The admin SPA SHALL expose a `/settings/notifications` page gated by the `notification:manage` permission that organises webhook endpoints, subscriptions, and deliveries into three tabs.

#### Scenario: Permission gates the menu

- **WHEN** a signed-in user holds `notification:manage`
- **THEN** the settings sub-menu shows a "通知" entry linking to `/settings/notifications`
- **AND** a user without `notification:manage` does not see the entry

#### Scenario: Three tabs under one page shell

- **WHEN** the user opens `/settings/notifications`
- **THEN** a single `PageContainer` renders Endpoints, Subscriptions, and Deliveries tabs
- **AND** switching a tab navigates between the sibling routes without re-mounting the page shell

### Requirement: Webhook endpoint management

The page SHALL let an operator create, edit, test, rotate the secret of, and delete webhook endpoints, revealing the `whsec_` signing secret exactly once on create and on rotate.

#### Scenario: One-time secret reveal on create

- **WHEN** an operator creates a webhook endpoint
- **THEN** a modal shows the full `whsec_` secret with a copy control and a "shown only once" warning
- **AND** after the modal closes no screen re-displays the plaintext secret

#### Scenario: Test sends a non-persisted probe

- **WHEN** an operator triggers Test on an endpoint
- **THEN** a signed `webhook.test` request is sent and its result is shown inline
- **AND** no delivery log row is created

#### Scenario: Delete warns about cascading subscriptions

- **WHEN** an operator deletes an endpoint that has subscriptions pointing at it
- **THEN** a confirmation states that those subscriptions are removed too

#### Scenario: Rotate warns about immediate invalidation

- **WHEN** an operator rotates an endpoint secret
- **THEN** a confirmation states that the old secret is invalidated immediately and receivers fail signature verification until updated

### Requirement: Subscription management

The page SHALL let an operator list, create, and delete subscriptions that bind a notification event type to either an email address or a webhook endpoint, optionally scoped to a single app.

#### Scenario: Create an email subscription

- **WHEN** an operator creates a subscription with channel email and a valid address
- **THEN** the subscription is created bound to that email and the chosen event type

#### Scenario: Create a webhook subscription

- **WHEN** an operator creates a subscription with channel webhook and selects an existing endpoint
- **THEN** the subscription is created bound to that endpoint

#### Scenario: Empty app scope means all apps

- **WHEN** the operator leaves the app field empty
- **THEN** the subscription is created matching all apps

### Requirement: Delivery log and redelivery

The page SHALL list deliveries with filterable status and endpoint, render the pending, sent, failed, and dead states with visually distinct badges, and let an operator manually redeliver a delivery while preserving its original webhook-id.

#### Scenario: Badges distinguish retryable from terminal

- **WHEN** a delivery is in the failed state
- **THEN** it shows a retryable badge together with its next retry time
- **AND** a dead delivery shows a terminal badge with no next retry time

#### Scenario: Redeliver preserves identity

- **WHEN** an operator redelivers a delivery
- **THEN** it is re-enqueued preserving the original webhook-id

#### Scenario: Arriving from an endpoint pre-filters the list

- **WHEN** the operator follows an endpoint's "view deliveries" action
- **THEN** the delivery list is pre-filtered to that endpoint

### Requirement: Rotate-secret affordance only for generic endpoints
The webhook endpoint list SHALL show the "rotate secret" action only for generic (Standard Webhooks) endpoints, because the server rejects rotation for IM providers whose signing secret is user-owned, so the action MUST NOT be offered where it would always fail.

#### Scenario: Generic endpoint offers rotation
- **WHEN** an operator views a generic webhook endpoint row
- **THEN** the "rotate secret" action is shown

#### Scenario: IM provider endpoint hides rotation
- **WHEN** an operator views a feishu, slack, dingtalk, or discord endpoint row
- **THEN** the "rotate secret" action is not shown
