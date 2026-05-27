# registration-policy

## ADDED Requirements

### Requirement: Server SHALL persist registration_policy as a singleton row

The server SHALL store registration settings in a `registration_policy` table with a single row `id=1`. On first boot the server SHALL insert defaults: all flags `false`, `require_email_verify=true`, `self_register_default_role_id` pointing to the `viewer` role, `self_register_require_approval=true`, `allowed_email_domains=[]`. The server SHALL expose `GET /api/v1/auth/registration-policy` and `PUT /api/v1/auth/registration-policy`, both requiring `auth:manage`.

#### Scenario: Defaults are seeded on first boot

- **GIVEN** the server starts with an empty `registration_policy` table
- **WHEN** startup completes
- **THEN** exactly one row exists with `id=1`
- **AND** all `allow_self_register_*` flags are `false`
- **AND** `require_email_verify=true` and `self_register_require_approval=true`
- **AND** `self_register_default_role_id` references the seeded `viewer` role

#### Scenario: Non-owner cannot update policy

- **GIVEN** a `publisher` session
- **WHEN** the client PUTs `/api/v1/auth/registration-policy`
- **THEN** the response is `403` with `type=missing_permission` and `required_permission: "auth:manage"`

### Requirement: Server SHALL gate email self-registration on policy

The server SHALL expose `POST /api/v1/auth/register { email, display_name, password }`. The handler SHALL reject with `410 Gone` `type=registration_disabled` when `policy.allow_self_register_email=false`. When enabled, the handler SHALL reject with `422` `type=email_domain_not_allowed` if `allowed_email_domains` is non-empty and the email's domain is not present. On success, the handler SHALL create the user with status determined by the policy: `pending_verify` if `require_email_verify=true`, else `pending_approval` if `require_approval=true`, else `active`.

#### Scenario: Disabled self-register blocks the endpoint

- **GIVEN** `policy.allow_self_register_email=false`
- **WHEN** a client POSTs `/api/v1/auth/register`
- **THEN** the response is `410` with `type=registration_disabled`
- **AND** no `user` row is created

#### Scenario: Domain whitelist filters non-matching emails

- **GIVEN** `policy.allow_self_register_email=true` and `allowed_email_domains=["example.com"]`
- **WHEN** a client POSTs `/api/v1/auth/register { email: "x@other.com", ... }`
- **THEN** the response is `422` with `type=email_domain_not_allowed`

#### Scenario: Policy with verify+approval yields pending_verify

- **GIVEN** policy `allow_self_register_email=true, require_email_verify=true, require_approval=true`
- **WHEN** a valid registration is accepted
- **THEN** the new user has `status='pending_verify'` and `email_verified=false`
- **AND** the response is `200 OK { next: 'verify_email' }`
- **AND** an `EmailVerify` `account_token` exists for the new user
- **AND** an `email_verify` mail was dispatched

### Requirement: Server SHALL implement email verify endpoint

The server SHALL expose `POST /api/v1/auth/verify-email { token }` and `GET /api/v1/auth/verify-email/info?token=` (returns email + expires_at). On successful verify, the user's `email_verified` SHALL flip to `true`, and based on `policy.require_approval`: `true` → set `status='pending_approval'` + INSERT `user_role(default_role)` + write session; `false` → set `status='active'` + INSERT `user_role` + write session. The server SHALL also expose `POST /api/v1/auth/verify-email/resend { email }` which always returns `200` and only acts when the email matches an unverified user (enumeration defense per ④).

#### Scenario: Verify with approval policy

- **GIVEN** policy `require_approval=true` and a user with `status='pending_verify'`, `email_verified=false`
- **WHEN** the user POSTs `/api/v1/auth/verify-email { token: <valid> }`
- **THEN** the response is `200 OK` with a session cookie
- **AND** the user's `email_verified=true` and `status='pending_approval'`
- **AND** a `user_role` row binds the user to the `policy.self_register_default_role_id`

#### Scenario: Verify without approval policy

- **GIVEN** policy `require_approval=false`
- **WHEN** a valid verify is submitted
- **THEN** the user's `status='active'`
- **AND** the response includes a session cookie

### Requirement: OAuth callback SHALL self-register when policy permits

When the OAuth callback (③ flow) reaches the "no existing identity_link, no email conflict" branch, the server SHALL consult `policy.allow_self_register_oauth`: when `false`, return `401` `type=oauth_registration_disabled`; when `true`, create the user with status mirroring `policy.require_approval` (true → pending_approval, false → active), set `email_verified=true` (OAuth verified email is trusted), insert `identity_link`, insert `user_role` to default role, write session. Domain whitelist applies the same as email path.

#### Scenario: Disabled OAuth self-register rejects new GitHub users

- **GIVEN** `policy.allow_self_register_oauth=false`
- **WHEN** a previously unknown GitHub user completes OAuth callback with a non-conflicting email
- **THEN** the response is `401` with `type=oauth_registration_disabled`
- **AND** no `user` or `identity_link` row is created

#### Scenario: Enabled OAuth self-register creates active user when no approval needed

- **GIVEN** policy `allow_self_register_oauth=true, require_approval=false`
- **WHEN** an unknown GitHub user completes callback
- **THEN** a new `user` row exists with `status='active'`, `email_verified=true`
- **AND** an `identity_link { kind: github, subject, user_id }` row exists
- **AND** a `user_role` row binds the default role
- **AND** the response is `302` redirect to `/` with a session cookie

### Requirement: Server SHALL provide pending_approval admin workflow

The server SHALL expose `GET /api/v1/users/pending-approval` (paginated, require `user:manage`), `POST /api/v1/users/:id/approve { role_id? }` (sets `status='active'`, optionally overrides default role), and `POST /api/v1/users/:id/reject { reason? }` (CASCADE delete user). All actions SHALL emit audit log events.

#### Scenario: Approve updates status and grants role

- **GIVEN** a pending_approval user U with the default role currently bound
- **WHEN** an Owner POSTs `/api/v1/users/U/approve { role_id: <publisher> }`
- **THEN** U's status is `active`
- **AND** U's `user_role` row binds the publisher role (overrides the previous default)
- **AND** an `user_approved` audit event is written

#### Scenario: Reject cascades delete

- **GIVEN** a pending_approval user U with associated `user_role`, `user_credentials`, and `account_token` rows
- **WHEN** an Owner POSTs `/api/v1/users/U/reject { reason: "spam" }`
- **THEN** all of U's user / user_role / user_credentials / identity_link / account_token rows are deleted
- **AND** an `user_rejected` audit event is written including `reason`

### Requirement: Admin SPA SHALL render Registration Policy card under Settings > Authentication

The Admin SPA SHALL add a "Registration Policy" Card on `/settings/authentication` (below the OAuth providers section from ③) containing a ProForm bound to GET/PUT `/api/v1/auth/registration-policy`. The form SHALL include: `allow_self_register_email` Switch, `require_email_verify` Switch (disabled when above is false), `allow_self_register_oauth` Switch, `self_register_default_role_id` role Select, `self_register_require_approval` Switch, and `allowed_email_domains` tag input. A "Save" button SHALL persist via PUT.

#### Scenario: require_email_verify is disabled when email register is off

- **GIVEN** the user opens `/settings/authentication`
- **WHEN** `allow_self_register_email` Switch is set to `false`
- **THEN** the `require_email_verify` Switch is visually disabled
- **AND** its current value is preserved (not auto-flipped)

#### Scenario: Banner warns when verify required but mail unconfigured

- **GIVEN** `policy.allow_self_register_email=true` and `policy.require_email_verify=true` and `mail_status.fallback_mode=true` (no active mail provider)
- **WHEN** the user opens `/settings/authentication`
- **THEN** a yellow Alert banner appears at the top warning that registration will be blocked without a configured Mail provider

### Requirement: Admin SPA SHALL render /register, /verify-email, and /awaiting-approval

The Admin SPA SHALL add three new public routes (`/register`, `/verify-email`) and one authenticated route (`/awaiting-approval`). The `_auth` layout route SHALL be extended so that any user with `status='pending_approval'` is redirected to `/awaiting-approval` (regardless of intended path). The `/awaiting-approval` page SHALL invalidate the `meQueryOptions` cache every 30 seconds so that admin approval is reflected automatically.

#### Scenario: Pending_approval user is funneled to awaiting-approval

- **GIVEN** the user's `me.status='pending_approval'`
- **WHEN** the user navigates to `/apps` or any other `_auth/*` route
- **THEN** the SPA redirects to `/awaiting-approval`
- **AND** the apps page is never rendered

#### Scenario: Approval is reflected without manual reload

- **GIVEN** the user is on `/awaiting-approval` and Owner approves the user in admin
- **WHEN** at most 30 seconds elapse (the polling interval)
- **THEN** the SPA navigates the user to `/`
- **AND** subsequent navigation works as a normal active user

#### Scenario: Disabled email self-register hides /register entry

- **GIVEN** `policy.allow_self_register_email=false`
- **WHEN** the user opens `/login`
- **THEN** the "Don't have an account? Register" link does not appear
- **AND** directly visiting `/register` redirects back to `/login` with an Alert

### Requirement: Admin SPA SHALL extend Users page with pending_approval workflow

The Admin SPA Users page (`/_auth/users`, from ④) SHALL extend its status filter to include `pending_approval`. Rows with `status='pending_approval'` SHALL expose "Approve" and "Reject" actions. Approve SHALL open a confirm Modal pre-filled with `policy.self_register_default_role_id` and allow overriding before POSTing. Reject SHALL open a confirm Modal with an optional `reason` text field.

#### Scenario: Approve opens role override modal

- **GIVEN** the Users page has a pending_approval row
- **WHEN** the user clicks "Approve" on that row
- **THEN** a Modal opens with a role Select pre-set to the policy default role
- **AND** clicking confirm POSTs `/approve` with the (possibly overridden) role_id
- **AND** on success the row's status changes to `active` and the page query refetches

### Requirement: user.email_verified SHALL be added with backfill for existing active users

The server SHALL add `email_verified: bool default false` to the `user` entity. During the migration to land this proposal, all existing rows with `status='active'` SHALL be backfilled to `email_verified=true` exactly once (using a startup-time hook with an idempotency marker). The historical `status='invited'` SHALL be mapped to `pending_verify` in the same migration.

#### Scenario: Active users are marked verified during migration

- **GIVEN** a deployed server with existing rows having `status='active'` (pre-migration)
- **WHEN** the server first boots after this proposal lands
- **THEN** every existing `status='active'` row has `email_verified=true`
- **AND** every existing `status='invited'` row has `status='pending_verify'`
- **AND** the migration hook is marked complete and does not re-run on subsequent boots
