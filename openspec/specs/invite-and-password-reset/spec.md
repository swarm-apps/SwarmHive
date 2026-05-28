# invite-and-password-reset Specification

## Purpose
TBD - created by archiving change add-invite-and-password-reset. Update Purpose after archive.
## Requirements
### Requirement: Server SHALL invite new users via tokenized email flow

The server SHALL expose `POST /api/v1/users/invite` requiring `user:manage` permission. The request body SHALL be `{ email, role_id, display_name? }`. The handler SHALL atomically create a new `user` row with `status='pending_verify'`, create a `user_role` row binding the role, create an `account_token` row with `purpose=Invite` and 72-hour `expires_at`, and dispatch a `user_invite` email containing the invite URL. The endpoint SHALL reject `role_id` referring to the `owner` role with `422` `type=cannot_invite_owner`.

#### Scenario: Owner invites a publisher

- **GIVEN** an Owner session and a target email `alice@example.com` not present in `user`
- **WHEN** the Owner POSTs `/api/v1/users/invite { email: "alice@example.com", role_id: <publisher> }`
- **THEN** the response is `200 OK` with `{ user_id, expires_at }`
- **AND** a new `user` row exists with `status='pending_verify'`
- **AND** a `user_role` row binds the new user to the publisher role
- **AND** an `account_token` row exists with `purpose=Invite` and `expires_at ≈ now + 72h`
- **AND** the Mailer received a `user_invite` envelope addressed to `alice@example.com`

#### Scenario: Inviting Owner role is rejected

- **WHEN** the Owner POSTs invite with `role_id` referring to the Owner role
- **THEN** the response is `422` with `type=cannot_invite_owner`
- **AND** no `user` row is created

### Requirement: Server SHALL provide invite acceptance endpoints

The server SHALL expose `GET /api/v1/auth/accept-invite/info?token=<plaintext>` (returns `{ email, display_name, role_name, inviter_name, expires_at }` for valid tokens; `410 Gone` for expired/consumed; `404` for unknown). The server SHALL expose `POST /api/v1/auth/accept-invite { token, password }` which sets the user's password (subject to strength validation per ①), flips `status='active'`, marks the token `consumed_at=now()`, and issues a session cookie.

#### Scenario: Valid invite token returns info

- **GIVEN** an unconsumed invite token whose `expires_at > now()`
- **WHEN** the client GETs `/api/v1/auth/accept-invite/info?token=<valid>`
- **THEN** the response is `200 OK` with email, display_name, role_name, inviter_name, expires_at
- **AND** the token's `consumed_at` is still `null` (read-only operation)

#### Scenario: Expired token returns 410

- **GIVEN** an invite token with `expires_at < now()`
- **WHEN** any client touches `/api/v1/auth/accept-invite/info?token=<expired>`
- **THEN** the response is `410 Gone` with `type=token_expired`

#### Scenario: Acceptance activates user and creates session

- **GIVEN** a valid invite token
- **WHEN** the client POSTs `/api/v1/auth/accept-invite { token, password: "<strong>" }`
- **THEN** the response is `200 OK` with a session cookie set
- **AND** the user's `status` is now `active`
- **AND** the user's `email_verified_at` is set to the current timestamp (clicking the invite link proves email reachability)
- **AND** the token's `consumed_at` is set
- **AND** a second POST with the same token returns `410` `type=token_already_consumed`

### Requirement: Server SHALL implement forgot-password with email enumeration defense and unverified-email block

The server SHALL expose `POST /api/v1/auth/forgot-password { email }` which SHALL always return `200 OK` with a generic acknowledgement regardless of whether the email exists in `user` and regardless of whether the user's email has been verified. When the email matches an active user **whose `email_verified_at` is non-null**, the server SHALL invalidate any existing unconsumed `PasswordReset` token for that user, create a new token with 1-hour expiry, and dispatch a `password_reset` email. When the email matches an unverified user, the server SHALL silently skip token creation and email dispatch, and write an audit log `password_reset_blocked_unverified`. When the email does not match any user, the server SHALL also skip. In all skip paths the handler SHALL still take approximately the same wall-clock time (≥ 150 ms) before responding to mitigate timing-based enumeration.

#### Scenario: Unknown email still returns 200

- **GIVEN** no user with email `ghost@example.com`
- **WHEN** the client POSTs `/api/v1/auth/forgot-password { email: "ghost@example.com" }`
- **THEN** the response is `200 OK`
- **AND** no `account_token` row is created
- **AND** no email is dispatched
- **AND** the response time is ≥ 150 ms (timing parity with the matched path)

#### Scenario: Known email creates one active reset token

- **GIVEN** an active user with email `bob@example.com`, `email_verified_at` set, and one previous unconsumed reset token
- **WHEN** the client POSTs `/api/v1/auth/forgot-password { email: "bob@example.com" }`
- **THEN** the previous reset token has `consumed_at` set (invalidated)
- **AND** exactly one new unconsumed reset token exists with `expires_at ≈ now + 1h`
- **AND** the Mailer received a `password_reset` envelope addressed to bob

#### Scenario: Unverified email is silently blocked

- **GIVEN** an active user with email `owner@example.com` and `email_verified_at = NULL`
- **WHEN** the client POSTs `/api/v1/auth/forgot-password { email: "owner@example.com" }`
- **THEN** the response is `200 OK` with the same generic message
- **AND** no `account_token` row is created
- **AND** no email is dispatched
- **AND** the server writes an audit log entry with action `password_reset_blocked_unverified`
- **AND** the response time is ≥ 150 ms (timing parity)

### Requirement: Server SHALL implement reset-password endpoint that revokes prior sessions

The server SHALL expose `GET /api/v1/auth/reset-password/info?token=<plaintext>` (returns `{ email, expires_at }`; `410` for invalid) and `POST /api/v1/auth/reset-password { token, password }` which validates the token, applies the new password (with strength check per ①), marks the token consumed, deletes ALL existing `session` rows for that user, issues a new session for the current request, and writes an audit log `password_reset_completed`.

#### Scenario: Reset invalidates all prior sessions

- **GIVEN** user B has 3 active `session` rows and a valid reset token
- **WHEN** B POSTs `/api/v1/auth/reset-password { token, password }`
- **THEN** the response is `200 OK` with a fresh session cookie
- **AND** the original 3 session rows are deleted from DB
- **AND** the current request's session row is created

### Requirement: Server SHALL store one-time tokens as hashed values with lookup index

The server SHALL store every `account_token.token_hash` as an argon2 hash of the plaintext token. The plaintext SHALL never be persisted. A `token_lookup` column SHALL hold the first 16 bytes of `sha256(plaintext)` base64-encoded; combined with `(purpose, token_lookup)` index it enables fast lookup before argon2 verification.

#### Scenario: DB dump does not leak plaintext tokens

- **GIVEN** any row in `account_token`
- **WHEN** an attacker inspects the row
- **THEN** the `token_hash` is an argon2 hash (starts with `$argon2id$`)
- **AND** no plaintext token value is stored anywhere in the row
- **AND** the `token_lookup` value cannot be reversed to plaintext (one-way hash)

### Requirement: Admin SPA SHALL render forgot-password / reset-password / accept-invite pages

The Admin SPA SHALL add three public top-level routes: `/forgot-password` (email submission with generic success message), `/reset-password?token=...` (token info pre-check + new-password form), `/accept-invite?token=...` (token info pre-check + welcome card + set-password form). All three pages SHALL render `<Trans>`-wrapped strings and use the same password strength feedback as `/setup` (per ①).

#### Scenario: Forgot-password always shows generic message

- **WHEN** the user submits any email on `/forgot-password`
- **THEN** the page displays a localized message similar to "If that email is registered, we sent a reset link"
- **AND** the response time is consistent regardless of email validity (server-side timing parity)

#### Scenario: Accept-invite shows welcome context

- **GIVEN** the user opens `/accept-invite?token=<valid>` for an invite from Owner targeting `alice@example.com` for publisher role
- **THEN** the page displays a welcome card showing inviter name, target role, and expiry
- **AND** an email field is shown as read-only with `alice@example.com`
- **AND** submitting a strong password results in navigation to `/`

#### Scenario: Expired token surfaces clear error UI

- **WHEN** the user opens `/reset-password` or `/accept-invite` with an expired token
- **THEN** the page displays a friendly error explaining the token is no longer valid
- **AND** the page suggests requesting a fresh invite/reset from the appropriate channel

### Requirement: Admin SPA SHALL render minimal Users page with invite drawer

The Admin SPA SHALL add `/_auth/users` (permission-gated on `user:manage`) showing a ProTable of users with columns: email, display_name, role(s), status (Tag: active/disabled/pending_verify), created_at. The page SHALL include an "Invite member" button opening a ProDrawerForm with email (double-entered for confirmation), role select (excluding Owner), and optional display_name. A "Resend invite" action SHALL be available on rows with `status=pending_verify`.

#### Scenario: Invite drawer requires double-entered email

- **WHEN** the user opens the Invite drawer
- **THEN** two email fields are shown
- **AND** the submit button is disabled until both fields contain the same value

#### Scenario: Resend invite is only visible for pending_verify rows

- **GIVEN** the Users table contains one `active` row and one `pending_verify` row
- **WHEN** the user inspects the actions column
- **THEN** only the `pending_verify` row exposes a "Resend invite" action

#### Scenario: Owner role is absent from the invite role select

- **WHEN** the user opens the role select in the Invite drawer
- **THEN** the options include only roles other than Owner (e.g. admin, publisher, viewer)

### Requirement: Server SHALL implement owner email self-verification endpoints

The server SHALL expose three endpoints supporting owner-initiated email verification, all backed by the same `account_token` table with `purpose=EmailVerify`:

1. `POST /api/v1/users/me/verify-email/send` (authenticated session required) SHALL invalidate any active `EmailVerify` token for the current user, create a fresh token with 24-hour expiry, and dispatch an `email_verify` envelope through `Mailer::send`. The endpoint SHALL reject calls when the user's `email_verified_at` is already set (`422 type=email_already_verified`). The endpoint SHALL reject calls when the active mailer transport is `console` / fallback (`422 type=mail_not_configured`, body MUST include `expected_next_step: "/settings/mail"`). The endpoint SHALL rate-limit to one send per user per 60 seconds (`429 type=rate_limited`).

2. `GET /api/v1/auth/verify-email/info?token=<plaintext>` SHALL be a public read-only endpoint returning `{ email, expires_at }` for valid tokens; `410 Gone` for expired or consumed; `404` for unknown. It SHALL NOT mark the token consumed.

3. `POST /api/v1/auth/verify-email { token }` SHALL be a public endpoint (no session required — the token is itself the credential) that validates the token, sets the user's `email_verified_at = now()` (only if currently NULL), marks the token consumed, and writes an audit log `email_verified`. It SHALL return `200 OK`.

#### Scenario: Verify send rejects when already verified

- **GIVEN** an authenticated user with `email_verified_at` already set
- **WHEN** the client POSTs `/api/v1/users/me/verify-email/send`
- **THEN** the response is `422` with `type=email_already_verified`
- **AND** no new `account_token` row is created
- **AND** no email is dispatched

#### Scenario: Verify send rejects under ConsoleMailer fallback

- **GIVEN** an authenticated user with `email_verified_at = NULL` and `mailer.kind() == "console"`
- **WHEN** the client POSTs `/api/v1/users/me/verify-email/send`
- **THEN** the response is `422` with `type=mail_not_configured`
- **AND** the response body includes `expected_next_step: "/settings/mail"`
- **AND** no `account_token` row is created

#### Scenario: Verify send enforces 60-second rate limit

- **GIVEN** a user who successfully POSTed `/api/v1/users/me/verify-email/send` 30 seconds ago
- **WHEN** the same user POSTs `/api/v1/users/me/verify-email/send` again
- **THEN** the response is `429` with `type=rate_limited`
- **AND** the previous active token is unchanged

#### Scenario: Consuming a verify token sets email_verified_at

- **GIVEN** a valid unconsumed `EmailVerify` token for user `owner@example.com` whose `email_verified_at` is currently NULL
- **WHEN** any unauthenticated client POSTs `/api/v1/auth/verify-email { token }`
- **THEN** the response is `200 OK`
- **AND** the user's `email_verified_at` is now set to the current timestamp
- **AND** the token's `consumed_at` is set
- **AND** an audit log entry `email_verified` is written
- **AND** a second POST with the same token returns `410 type=token_already_consumed`

#### Scenario: Verify info is read-only

- **GIVEN** a valid unconsumed `EmailVerify` token
- **WHEN** any client GETs `/api/v1/auth/verify-email/info?token=<valid>`
- **THEN** the response is `200 OK` with `{ email, expires_at }`
- **AND** the token's `consumed_at` is still NULL

### Requirement: Admin SPA SHALL render unverified-email banner with mail-status awareness

The Admin SPA SHALL render a persistent (non-dismissible) banner inside the `_auth` ProLayout whenever the current user's `email_verified_at` is NULL. The banner SHALL switch its message and action based on `mailStatusQueryOptions`:

- When `mailStatus.fallback_mode == false` (an SMTP provider is active), the banner SHALL show "your email is not verified" copy with a "Resend verification email" action that calls `POST /api/v1/users/me/verify-email/send`.
- When `mailStatus.fallback_mode == true` (ConsoleMailer fallback), the banner SHALL show "email is not configured, please configure SMTP first" copy with an action linking to `/settings/mail`. The "Resend verification" action SHALL be hidden.

The banner SHALL disappear immediately when `me.email_verified_at` becomes non-null (after a successful verify or after a re-fetch of `meQuery`).

#### Scenario: Banner shows verify CTA when SMTP active and email unverified

- **GIVEN** a logged-in user with `email_verified_at = NULL`
- **AND** `GET /api/v1/mail/status` returns `{ transport: "smtp", fallback_mode: false }`
- **WHEN** the user lands on any `/_auth/*` page
- **THEN** the top of the ProLayout displays a yellow banner with text containing "尚未验证" or equivalent localized string
- **AND** the banner has a primary action "重发验证邮件" that triggers `POST /api/v1/users/me/verify-email/send`
- **AND** the banner has no close button

#### Scenario: Banner pivots to SMTP-config CTA under fallback

- **GIVEN** a logged-in user with `email_verified_at = NULL`
- **AND** `GET /api/v1/mail/status` returns `{ transport: "console", fallback_mode: true }`
- **WHEN** the user lands on any `/_auth/*` page
- **THEN** the banner copy mentions "邮件未配置" or equivalent
- **AND** the primary action navigates to `/settings/mail` (no verify call is issued)

#### Scenario: Banner disappears after successful verify

- **GIVEN** a user who just POSTed `/api/v1/auth/verify-email` with a valid token
- **WHEN** the SPA re-fetches `meQueryOptions` (either via explicit invalidation or on next route load)
- **THEN** `me.email_verified_at` is non-null
- **AND** the banner no longer renders

### Requirement: Admin SPA SHALL render verify-email landing page

The Admin SPA SHALL add a public top-level route `/verify-email?token=<plaintext>` that calls `verify-email/info` for pre-flight token validation, displays a welcome card showing the email being verified and the expiry, and exposes a single primary button to consume the token via `POST /api/v1/auth/verify-email`. On success the page SHALL navigate to `/` with `replace: true` (so the token URL does not enter back history). On 410/404 the page SHALL display a friendly error explaining the link is no longer valid and suggest re-requesting verification from the in-app banner.

#### Scenario: Valid verify token shows email and primary button

- **GIVEN** a user opening `/verify-email?token=<valid>` for an email `owner@example.com`
- **WHEN** the page loads
- **THEN** the page displays the email `owner@example.com` (read-only) and the token expiry
- **AND** a single primary button labelled "Verify" / "验证邮箱" is shown
- **AND** no password input is rendered (verify is a single-tap action)

#### Scenario: Successful verify navigates to root with replace

- **GIVEN** a valid verify token displayed on `/verify-email`
- **WHEN** the user clicks the primary button
- **THEN** `POST /api/v1/auth/verify-email` returns `200 OK`
- **AND** the SPA navigates to `/` with `replace: true`
- **AND** pressing the browser back button does not return to the token URL

