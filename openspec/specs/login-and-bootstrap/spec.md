# login-and-bootstrap Specification

## Purpose
TBD - created by archiving change add-login-and-owner-bootstrap-ui. Update Purpose after archive.
## Requirements
### Requirement: Server SHALL bootstrap the first Owner via a tokenless web flow

The server SHALL serve `GET /api/v1/setup/info` returning `{ needs_bootstrap: bool, locked_email: Option<String> }`, where `needs_bootstrap` is `true` iff the `user` table is empty. The server SHALL accept `POST /api/v1/setup` with body `{ email, display_name, password }` (no `token` field) only while `needs_bootstrap` is `true`. After the first successful POST, subsequent POSTs SHALL return `410 Gone` with `application/problem+json` `type=bootstrap_already_complete`.

#### Scenario: Empty DB allows tokenless setup

- **GIVEN** the `user` table is empty
- **WHEN** the client POSTs `{ email, display_name, password }` to `/api/v1/setup` with no `token` field
- **THEN** the response is `200 OK` with a session cookie set
- **AND** a user row is created with the default Owner role bound

#### Scenario: Setup is closed after first Owner

- **GIVEN** a successful setup has already completed
- **WHEN** any client POSTs to `/api/v1/setup`
- **THEN** the response is `410 Gone` with `Content-Type: application/problem+json` and `type=bootstrap_already_complete`

### Requirement: Server SHALL honor SWARMHIVE_BOOTSTRAP_OWNER_EMAIL when set

When the environment variable `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL` is set at server startup, `GET /api/v1/setup/info` SHALL include the value in `locked_email`, and `POST /api/v1/setup` SHALL reject any request whose `email` field differs from the locked value with `422 Unprocessable Entity` and `type=bootstrap_email_mismatch`.

#### Scenario: ENV locks the bootstrap email

- **GIVEN** server started with `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL=owner@example.com`
- **WHEN** the client GETs `/api/v1/setup/info`
- **THEN** the response body contains `locked_email: "owner@example.com"`

#### Scenario: Locked email mismatch is rejected

- **GIVEN** server started with `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL=owner@example.com`
- **WHEN** the client POSTs `/api/v1/setup` with `email: "attacker@evil.com"`
- **THEN** the response is `422` with `type=bootstrap_email_mismatch`
- **AND** the response body includes the expected email so the client can correct itself

### Requirement: Server SHALL enforce account-level login lockout

The server SHALL maintain a `user_login_attempts` row per user tracking `failed_count`, `last_failed_at`, and `locked_until`. After **5** consecutive failed `/api/v1/auth/login` attempts within the active window, the server SHALL set `locked_until = now() + 30 minutes`. Login requests received during lockout SHALL return `410 Gone` with `Content-Type: application/problem+json` and `type=account_locked_until` whose body contains the `locked_until` ISO-8601 timestamp. A successful login SHALL clear the row.

#### Scenario: Five failures trigger 30-minute lock

- **GIVEN** a user with no existing failed attempts
- **WHEN** the client POSTs `/api/v1/auth/login` with the wrong password five times in a row
- **THEN** the sixth login attempt receives `410 Gone` with `type=account_locked_until`
- **AND** the body includes `locked_until` set to approximately `now() + 30 minutes`

#### Scenario: Successful login clears the lock

- **GIVEN** a user has 3 prior failed attempts on record
- **WHEN** the user POSTs `/api/v1/auth/login` with the correct password
- **THEN** the response is `200 OK` with a session cookie set
- **AND** the `user_login_attempts` row for that user is deleted (or `failed_count` reset to 0)

### Requirement: Server SHALL enforce password strength on setup

The server SHALL validate the `password` field of `POST /api/v1/setup` (and any future password-set endpoint) against: minimum 12 characters, at least 3 of 4 character classes (uppercase / lowercase / digit / special), and rejection of any password matching the bundled weak-password dictionary (top-100). Failures SHALL return `422` with `application/problem+json` `type=password_too_weak` and a `detail` enumerating which rule was violated.

#### Scenario: Short password is rejected

- **WHEN** the client POSTs setup with `password: "Sh0rt!"`
- **THEN** the response is `422` with `type=password_too_weak` and `detail` mentioning length

#### Scenario: Common weak password is rejected

- **WHEN** the client POSTs setup with `password: "Password123!"`
- **THEN** the response is `422` with `type=password_too_weak` and `detail` mentioning the weak-password dictionary

### Requirement: Admin SPA SHALL route the user to /setup or /login based on bootstrap state

The Admin SPA root route SHALL, in `beforeLoad`, fetch `GET /api/v1/setup/info` (cached, 60s stale time) and route accordingly: `needs_bootstrap: true` AND current path != `/setup` → redirect to `/setup`; `needs_bootstrap: false` AND current path === `/setup` → redirect to `/login`. The redirect SHALL use `replace: true` to avoid history pollution.

#### Scenario: Empty deployment routes to /setup

- **GIVEN** the SPA boots against a server reporting `needs_bootstrap: true`
- **WHEN** the user opens the SPA at `/`
- **THEN** the URL is replaced with `/setup`
- **AND** no `/api/v1/auth/me` request is issued (the auth guard is short-circuited)

#### Scenario: Configured deployment routes to /login

- **GIVEN** the SPA boots against a server reporting `needs_bootstrap: false`
- **WHEN** the user opens the SPA at `/setup`
- **THEN** the URL is replaced with `/login`

### Requirement: Admin SPA SHALL render real /login and /setup forms

The Admin SPA SHALL replace the previous `/login` placeholder Card with a real ProForm (email + password + remember-me + disabled "Forgot password" link) that submits to `POST /api/v1/auth/login` via `fetchClient`. The Admin SPA SHALL implement `/setup` as a new top-level route rendering a ProForm (email + display_name + password + confirm) where the email field is `disabled` and pre-filled when the server reports `locked_email`. Both forms SHALL wrap every user-visible string in `<Trans>` macros.

#### Scenario: Login form posts and navigates to next

- **GIVEN** the user lands on `/login?next=/apps`
- **WHEN** the user submits valid credentials
- **THEN** the SPA sends `POST /api/v1/auth/login`
- **AND** on success navigates to `/apps`

#### Scenario: Locked email pre-fills setup form

- **GIVEN** the server reports `locked_email: "owner@example.com"`
- **WHEN** the user opens `/setup`
- **THEN** the email field is disabled and pre-filled with `owner@example.com`
- **AND** submitting any other email is impossible via the UI

#### Scenario: Lockout error surfaces countdown UI

- **GIVEN** the user has triggered the 5-failure lockout
- **WHEN** the user submits `/login` once more
- **THEN** the SPA receives the `410 account_locked_until` problem+json
- **AND** the form displays a localized message including the absolute lockout-until time

