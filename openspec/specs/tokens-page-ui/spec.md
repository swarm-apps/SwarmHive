# tokens-page-ui Specification

## Purpose
TBD - created by archiving change add-tokens-page-ui. Update Purpose after archive.
## Requirements
### Requirement: Admin SHALL list the current user's tokens

The Admin SPA SHALL provide a top-level `/tokens` page listing the signed-in user's own tokens via `GET /api/v1/tokens`, showing for each: name, prefix, kind (PAT / API), the permission set (for API tokens), `last_used_at`, `expires_at`, and a derived status of active / revoked / expired. The list SHALL be visible to any signed-in user (no special permission), since it only returns the caller's own tokens.

#### Scenario: List shows own tokens with derived status

- **WHEN** a signed-in user opens `/tokens`
- **THEN** their own tokens are listed with name, prefix, kind, permissions (API), last-used, expiry
- **AND** each row shows a status of active, revoked (when `revoked_at` is set), or expired (when `expires_at` is past)

### Requirement: Admin SHALL create tokens with a permission subset

The page SHALL let a user holding `token:manage` create a token via `POST /api/v1/tokens`: choosing kind PAT or API, a name, and an optional `expires_at`. For kind API the user SHALL select a permission subset whose options are the creator's own permissions (a token can never exceed the creator's permissions); for kind PAT the `permissions` field SHALL be omitted (the PAT inherits the owner's live permissions). The create control SHALL be hidden when the user lacks `token:manage` (hide-not-disable).

#### Scenario: Create an API token scoped to a permission subset

- **GIVEN** a user holding `token:manage`
- **WHEN** they create a token with kind API, a name, and selected permissions (e.g. `release:publish`, `artifact:upload`)
- **THEN** the request body carries `kind: "api"` and the chosen `permissions`
- **AND** the permission options offered were the creator's own permissions

#### Scenario: Create a PAT omits permissions

- **WHEN** the user creates a token with kind PAT
- **THEN** the request body omits `permissions` (PAT inherits the owner's live permissions)

#### Scenario: Create control hidden without token:manage

- **GIVEN** a user lacking `token:manage`
- **WHEN** they open `/tokens`
- **THEN** the create-token control is not shown
- **AND** they can still see and revoke their own tokens

### Requirement: Admin SHALL reveal the plaintext token exactly once

On successful creation the page SHALL display the plaintext token (from `CreateTokenResponse.token`) exactly once, with a copy action and a warning that it cannot be retrieved again. The plaintext SHALL NOT be persisted to the query cache or shown in the list (which shows only the prefix).

#### Scenario: Plaintext shown once after creation

- **WHEN** a token is created successfully
- **THEN** a modal shows the plaintext token with a copy button and a "save it now" warning
- **AND** after the modal is dismissed the plaintext is no longer available; the list shows only the prefix

### Requirement: Admin SHALL revoke a token

The page SHALL let the user revoke a listed (own) token via `DELETE /api/v1/tokens/:id` behind a confirmation. Revocation SHALL be idempotent (re-revoking a revoked token is not an error) and the list SHALL refresh afterward.

#### Scenario: Revoke a token

- **WHEN** the user confirms revoking one of their tokens
- **THEN** `DELETE /api/v1/tokens/:id` is called and the list refreshes
- **AND** the token now shows status revoked

