# registration-policy

> **Rebased 2026-06-10**：requirements 与真实 ship 的 ①②③④ 对齐——`Invited` 改名 `Provisioned`
> (语义纯净,统得起 invite + self-register;非 `pending_verify`),verify 信号用既有 `email_verified_at`
> (非新加 bool、非 backfill),verify-email 端点为**扩展** ④ 现有实现。

## ADDED Requirements

### Requirement: Server SHALL persist registration_policy as a singleton row

The server SHALL store registration settings in a `registration_policy` table with a single row `id=1`. On first boot the server SHALL insert defaults: all `allow_self_register_*` flags `false`, `require_email_verify=true`, `self_register_default_role_id` pointing to the `viewer` role, `self_register_require_approval=true`, `allowed_email_domains=[]`. The server SHALL expose `GET /api/v1/auth/registration-policy` and `PUT /api/v1/auth/registration-policy`, both requiring the existing `auth:manage` permission.

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

### Requirement: user.status SHALL rename Invited to Provisioned and add PendingApproval

The server SHALL rename the `UserStatus::Invited` variant to `Provisioned` (string value `"invited"` → `"provisioned"`) and add a `PendingApproval` variant, yielding `{Active, Disabled, Provisioned, PendingApproval}`. `Provisioned` is the umbrella status for "account exists, awaiting confirmation (invite acceptance or email verification)", covering both the invite (④) and self-register (⑤) flows. The server SHALL NOT introduce a `pending_verify` status, and SHALL NOT add an `email_verified` boolean column — verification state remains the existing `user.email_verified_at: Option<DateTimeUtc>` (`NULL` = unverified). The rename requires a one-time data migration `UPDATE "user" SET status='provisioned' WHERE status='invited'`, executed as raw SQL **before any User entity read** (stale `'invited'` values would otherwise fail enum deserialization). The migration is naturally idempotent (no marker table). No `email_verified_at` backfill SHALL occur.

#### Scenario: Existing invited rows are migrated to provisioned

- **GIVEN** a deployed server with existing rows having `status='invited'` and Owner setups whose `email_verified_at IS NULL`
- **WHEN** the server first boots after this proposal lands
- **THEN** every previously-`'invited'` row now has `status='provisioned'`
- **AND** the `user.status` enum also accepts a new `pending_approval` value
- **AND** no existing row's `email_verified_at` is changed (Owner opt-in NULL is preserved)
- **AND** re-running the boot migration is a no-op (idempotent)

### Requirement: Server SHALL gate email self-registration on policy

The server SHALL expose `POST /api/v1/auth/register { email, display_name, password }` (flat `routes/register.rs`, public). The handler SHALL reject with `410 Gone` `type=registration_disabled` when `policy.allow_self_register_email=false`; with `422 type=email_already_taken` when the email is occupied; with `422 type=email_domain_not_allowed` when `allowed_email_domains` is non-empty and the domain is absent; and with `422 type=password_too_weak` (reusing ①'s validator) for weak passwords. On success it SHALL create `user(status=Provisioned, email_verified_at=NULL)` + `user_credentials` + `user_role(default_role)`, then branch on policy.

#### Scenario: Disabled self-register blocks the endpoint

- **GIVEN** `policy.allow_self_register_email=false`
- **WHEN** a client POSTs `/api/v1/auth/register`
- **THEN** the response is `410` with `type=registration_disabled`
- **AND** no `user` row is created

#### Scenario: Domain whitelist filters non-matching emails

- **GIVEN** `policy.allow_self_register_email=true` and `allowed_email_domains=["example.com"]`
- **WHEN** a client POSTs `/api/v1/auth/register { email: "x@other.com", ... }`
- **THEN** the response is `422` with `type=email_domain_not_allowed`

#### Scenario: Policy with verify yields Provisioned + verify mail

- **GIVEN** policy `allow_self_register_email=true, require_email_verify=true`
- **WHEN** a valid registration is accepted
- **THEN** the new user has `status='provisioned'` and `email_verified_at IS NULL`
- **AND** the response is `200 OK { next: 'verify_email' }` (no session)
- **AND** an `EmailVerify` `account_token` exists for the new user
- **AND** an `email_verify` mail was dispatched

#### Scenario: No-verify + approval yields PendingApproval directly

- **GIVEN** policy `require_email_verify=false, require_approval=true`
- **WHEN** a valid registration is accepted
- **THEN** the new user has `status='pending_approval'`
- **AND** the response is `200 OK { next: 'pending_approval' }` with a session cookie

### Requirement: verify-email SHALL transition Provisioned users on consume (extends ④)

The existing public `POST /api/v1/auth/verify-email { token }` (from ④, which sets `email_verified_at` and does not touch `status`) SHALL be extended: when the consuming user's current `status='provisioned'`, the handler SHALL transition status per `policy.require_approval` (`true` → `pending_approval`, `false` → `active`) and write a session. When the user's status is already `active` (in-app banner verification), behaviour SHALL be unchanged (timestamp only, no transition). The default role binding is created at `/register` time, so verify-email SHALL NOT re-bind roles. A new public `POST /api/v1/auth/verify-email/resend { email }` SHALL always return `200` and only act on a user whose `email_verified_at IS NULL` (enumeration defense), since pre-verification self-registrants have no session for the authed `me/verify-email/send`.

#### Scenario: Provisioned user with approval policy → pending_approval

- **GIVEN** policy `require_approval=true` and a user with `status='provisioned'`, `email_verified_at IS NULL`
- **WHEN** the user POSTs `/api/v1/auth/verify-email { token: <valid> }`
- **THEN** the response is `200 OK` with a session cookie
- **AND** the user's `email_verified_at` is non-NULL and `status='pending_approval'`

#### Scenario: Provisioned user without approval policy → active

- **GIVEN** policy `require_approval=false` and a user with `status='provisioned'`
- **WHEN** a valid verify is submitted
- **THEN** the user's `status='active'` and the response includes a session cookie

#### Scenario: Already-active banner verify does not change status

- **GIVEN** a user with `status='active'` and `email_verified_at IS NULL`
- **WHEN** the user verifies via the in-app banner token
- **THEN** `email_verified_at` becomes non-NULL
- **AND** the user's `status` remains `active`

### Requirement: OAuth callback SHALL self-register when policy permits

When the OAuth callback (③ flow, `routes/oauth.rs`) reaches the "no existing identity_link, no email conflict" branch (currently a hard `401 oauth_registration_disabled`), the server SHALL consult `policy.allow_self_register_oauth`: when `false`, keep `401 type=oauth_registration_disabled`; when `true`, validate the GitHub verified email's domain against `allowed_email_domains`, then create `user(status=active|pending_approval per require_approval, email_verified_at=now())` + `identity_link` + `user_role(default_role)`, write a session, and `302` to `/` (or `/awaiting-approval`). Domain mismatch SHALL `302` to `/login?oauth_error=domain_not_allowed`.

#### Scenario: Disabled OAuth self-register rejects new GitHub users

- **GIVEN** `policy.allow_self_register_oauth=false`
- **WHEN** a previously unknown GitHub user completes OAuth callback with a non-conflicting email
- **THEN** the response is `401` with `type=oauth_registration_disabled`
- **AND** no `user` or `identity_link` row is created

#### Scenario: Enabled OAuth self-register creates active user when no approval needed

- **GIVEN** policy `allow_self_register_oauth=true, require_approval=false`
- **WHEN** an unknown GitHub user (verified email in-whitelist) completes callback
- **THEN** a new `user` row exists with `status='active'`, `email_verified_at` non-NULL
- **AND** an `identity_link { provider: github, subject, user_id }` row exists
- **AND** a `user_role` row binds the default role
- **AND** the response is `302` redirect to `/` with a session cookie

### Requirement: Server SHALL provide pending_approval admin workflow

The server SHALL extend `routes/users.rs` with `GET /api/v1/users/pending-approval` (paginated, require `user:manage`), `POST /api/v1/users/:id/approve { role_id? }` (sets `status='active'`, optionally overrides the bound role), and `POST /api/v1/users/:id/reject { reason? }` (CASCADE delete user). All actions SHALL emit audit log events. The `role_id` Select reuses the existing `GET /api/v1/roles`.

#### Scenario: Approve updates status and grants role

- **GIVEN** a pending_approval user U with the default role currently bound
- **WHEN** an Owner POSTs `/api/v1/users/U/approve { role_id: <publisher> }`
- **THEN** U's status is `active`
- **AND** U's `user_role` row binds the publisher role (overrides the previous default)
- **AND** a `user_approved` audit event is written

#### Scenario: Reject cascades delete

- **GIVEN** a pending_approval user U with associated `user_role`, `user_credentials`, and `account_token` rows
- **WHEN** an Owner POSTs `/api/v1/users/U/reject { reason: "spam" }`
- **THEN** all of U's user / user_role / user_credentials / identity_link / account_token rows are deleted
- **AND** a `user_rejected` audit event is written including `reason`

### Requirement: Admin SPA SHALL render Registration Policy as a standalone settings page

The Admin SPA SHALL render the registration policy on a dedicated `/settings/registration` page (sidebar item "注册策略" under 设置; decided 2026-06-10, superseding the original "card under Settings › Authentication" plan) containing a ProForm bound to GET/PUT `/api/v1/auth/registration-policy`. The form SHALL include: `allow_self_register_email` Switch, `require_email_verify` Switch (disabled when above is false), `allow_self_register_oauth` Switch, `self_register_default_role_id` role Select (from `GET /api/v1/roles`), `self_register_require_approval` Switch, and `allowed_email_domains` tag input. A "Save" button SHALL persist via PUT. The Settings › Authentication page SHALL link to this page from an info Alert.

#### Scenario: require_email_verify is disabled when email register is off

- **GIVEN** the user opens Settings › 注册策略
- **WHEN** `allow_self_register_email` Switch is set to `false`
- **THEN** the `require_email_verify` Switch is visually disabled
- **AND** its current value is preserved (not auto-flipped)

#### Scenario: Banner warns when verify required but mail unconfigured

- **GIVEN** `policy.allow_self_register_email=true` and `policy.require_email_verify=true` and mail status is in fallback mode (no active mail provider)
- **WHEN** the user opens Settings › 注册策略
- **THEN** a yellow Alert banner appears at the top warning that registration will be blocked without a configured Mail provider

### Requirement: Admin SPA SHALL render /register and /awaiting-approval and reuse /verify-email

The Admin SPA SHALL add a public `/register` route and an authenticated `/awaiting-approval` route, and reuse the existing public `/verify-email` route (④) — extending it to follow ⑤'s `next` response. The `_auth` guard SHALL redirect any user with `status='pending_approval'` to `/awaiting-approval` (regardless of intended path). The `/awaiting-approval` page SHALL invalidate `meQueryOptions` every 30 seconds so admin approval is reflected automatically.

#### Scenario: Pending_approval user is funneled to awaiting-approval

- **GIVEN** the user's `me.status='pending_approval'`
- **WHEN** the user navigates to `/apps` or any other `_auth/*` route
- **THEN** the SPA redirects to `/awaiting-approval`
- **AND** the apps page is never rendered

#### Scenario: Approval is reflected without manual reload

- **GIVEN** the user is on `/awaiting-approval` and Owner approves the user
- **WHEN** at most 30 seconds elapse (the polling interval)
- **THEN** the SPA navigates the user to `/`

#### Scenario: Disabled email self-register hides /register entry

- **GIVEN** `policy.allow_self_register_email=false`
- **WHEN** the user opens `/login`
- **THEN** the "Don't have an account? Register" link does not appear
- **AND** directly visiting `/register` redirects back to `/login` with an Alert

### Requirement: Admin SPA SHALL provide a dedicated approvals page and member-management actions

The Admin SPA SHALL render the approval workflow on a dedicated `/users/approvals` page (sidebar 成员 › 注册审批; decided 2026-06-10, superseding inline row actions), backed by the paginated `GET /api/v1/users/pending-approval` (whose items SHALL include each user's roles). "Approve" SHALL open a Modal pre-filled with the role bound at registration time (the policy default) and allow overriding; "Reject" SHALL open a Modal with an optional `reason`. The members list (`/users/list`; `/users` redirects there so parent/child menu paths never collide) SHALL keep its `pending_approval` status filter, render only a "去审批" link for pending rows, and SHALL expose member-management actions for non-owner, non-self rows: change role (`PUT /users/{id}/role`, whole-binding replacement, owner excluded), disable (`POST /users/{id}/disable`, Active only, revokes all sessions), and enable (`POST /users/{id}/enable`, Disabled only).

#### Scenario: Approve opens role override modal on the approvals page

- **GIVEN** the approvals page lists a pending_approval row
- **WHEN** the user clicks "批准" on that row
- **THEN** a Modal opens with a role Select pre-set to the role bound at registration
- **AND** clicking confirm POSTs `/approve` with the (possibly overridden) role_id
- **AND** on success the row leaves the pending list and the members-list cache is invalidated

#### Scenario: Disabling a member revokes their sessions

- **GIVEN** an active non-owner member with a live session
- **WHEN** a `user:manage` holder POSTs `/api/v1/users/{id}/disable`
- **THEN** the response is `204`, the user's status is `disabled`
- **AND** the member's existing session returns `401` on the next request

#### Scenario: Owner and self are protected from member management

- **GIVEN** the owner account (or the caller themselves)
- **WHEN** a role change or disable is attempted against it
- **THEN** the response is `422` with `type=cannot-manage-owner` (or `cannot-manage-self`)
- **AND** the UI renders no management actions on owner/self rows
