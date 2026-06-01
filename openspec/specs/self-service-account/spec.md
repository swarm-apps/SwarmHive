# self-service-account Specification

## Purpose
TBD - created by archiving change add-self-service-account. Update Purpose after archive.
## Requirements
### Requirement: Server SHALL let an authenticated user update their own display name

The server SHALL expose `PATCH /api/v1/users/me { display_name }`, available to any authenticated **Active** principal (session or bearer) with no additional permission — the scope is always the caller. The handler SHALL trim `display_name` and reject with `422` when the trimmed value is empty or longer than 100 characters. On success it SHALL persist the new name and return the updated `User`.

#### Scenario: Owner updates their display name

- **GIVEN** an authenticated Active session
- **WHEN** the client PATCHes `/api/v1/users/me` with `{ "display_name": "New Name" }`
- **THEN** the response is `200` with the updated `User` whose `display_name` is `"New Name"`
- **AND** a subsequent `GET /api/v1/auth/me` reflects the new name

#### Scenario: Empty display name is rejected

- **GIVEN** an authenticated Active session
- **WHEN** the client PATCHes `/api/v1/users/me` with `{ "display_name": "   " }`
- **THEN** the response is `422`
- **AND** the persisted display name is unchanged

### Requirement: Server SHALL let an authenticated user change or set their own password

The server SHALL expose `PUT /api/v1/users/me/password { current_password?, new_password }`, available to any authenticated Active principal with no additional permission. When the caller HAS a `user_credentials` row, `current_password` SHALL be required and verified against the stored argon2 hash; a missing or wrong value SHALL return `422` with `type=current_password_incorrect`. When the caller has NO `user_credentials` row (OAuth-only account), the endpoint SHALL treat the request as "set password" and ignore `current_password`. In both cases `new_password` SHALL pass `validate_strong_password` (else `422` `type=password_too_weak`). On success the server SHALL, in a single transaction, upsert the credential and delete every persisted session for the user, then re-issue the current request's session, write an `audit_log` row with action `auth:password_changed`, and return `204`.

#### Scenario: Wrong current password is rejected

- **GIVEN** an authenticated Active session for a user who has a password
- **WHEN** the client PUTs `/api/v1/users/me/password` with a wrong `current_password`
- **THEN** the response is `422` with `type=current_password_incorrect`
- **AND** the stored credential is unchanged

#### Scenario: Weak new password is rejected

- **GIVEN** an authenticated Active session and a correct `current_password`
- **WHEN** the client PUTs `/api/v1/users/me/password` with `new_password` of `"short"`
- **THEN** the response is `422` with `type=password_too_weak`

#### Scenario: Successful change invalidates other sessions

- **GIVEN** the same user is logged in on two clients A and B
- **WHEN** client A PUTs `/api/v1/users/me/password` with a correct current and a strong new password
- **THEN** the response is `204`
- **AND** client A's next request still succeeds (current session re-issued)
- **AND** client B's next request returns `401` (its session was revoked)

#### Scenario: OAuth-only user sets a password

- **GIVEN** an authenticated Active user with a GitHub identity link and NO `user_credentials` row
- **WHEN** the client PUTs `/api/v1/users/me/password` with `new_password` set and no `current_password`
- **THEN** the response is `204`
- **AND** the user can subsequently authenticate via `POST /api/v1/auth/login` with the new password

### Requirement: Admin SPA SHALL consolidate personal account management under the profile page

The Admin SPA SHALL present all current-user account management on a single `/profile` page reached from the avatar dropdown, covering account info (email read-only, editable display name, email-verification status with resend), security (change/set password), and connected login methods (OAuth link/unlink). The settings menu SHALL no longer contain an "Account" entry, and the settings section SHALL be visible only to users holding at least one `*:manage` permission.

#### Scenario: Non-manager sees no settings menu

- **GIVEN** a logged-in user with no `*:manage` permission
- **WHEN** they view the admin layout
- **THEN** the sidebar does NOT show the "设置" menu
- **AND** the avatar dropdown's "个人资料" entry opens `/profile` containing account info, security, and login methods

#### Scenario: Visiting /settings without manage permission

- **GIVEN** a logged-in user with no `*:manage` permission
- **WHEN** they navigate directly to `/settings`
- **THEN** they are redirected to `/profile`

