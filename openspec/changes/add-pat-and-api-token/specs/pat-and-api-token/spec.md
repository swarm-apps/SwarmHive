# spec — pat-and-api-token

## ADDED Requirements

### Requirement: API Token entity is single table for PAT and API Token

The system SHALL store both Personal Access Tokens (PAT) and API Tokens in a single `api_token` table, distinguished by a `kind` column with values `'pat'` or `'api'`. The table SHALL include columns: `id` (uuid v7 primary key), `owner_user_id` (FK to user.id), `kind`, `name`, `prefix` (first 12 plaintext chars), `token_hash` (blake3 32-byte UNIQUE), `permissions` (JSONB array, NULL or non-NULL per kind), `last_used_at`, `expires_at`, `revoked_at`, `created_at`.

#### Scenario: PAT row has NULL permissions
- **WHEN** a PAT is created via `POST /api/v1/tokens` with `{kind: "pat"}` or via `POST /api/v1/auth/cli-token`
- **THEN** the inserted row has `kind='pat'` and `permissions IS NULL`

#### Scenario: API Token row has non-NULL permissions
- **WHEN** an API Token is created via `POST /api/v1/tokens` with `{kind: "api", permissions: [...]}`
- **THEN** the inserted row has `kind='api'` and `permissions` is a JSON array of `PermissionName` values

#### Scenario: token_hash uniqueness is enforced
- **WHEN** an INSERT would produce a duplicate `token_hash`
- **THEN** the operation fails with a unique-constraint error and no row is written

### Requirement: Token string format uses kind-prefixed base64url

The system SHALL emit token strings in the format `swhv_<kind>_<43char base64url>` where `<kind>` is `pat` or `api`, and the 43-char suffix is the URL-safe base64 (no padding) encoding of 32 cryptographically random bytes generated via `OsRng`.

#### Scenario: PAT string starts with `swhv_pat_`
- **WHEN** a PAT is created
- **THEN** the returned `token` string starts with `swhv_pat_` and is exactly 52 characters total

#### Scenario: API Token string starts with `swhv_api_`
- **WHEN** an API Token is created
- **THEN** the returned `token` string starts with `swhv_api_` and is exactly 52 characters total

#### Scenario: Server stores only blake3 hash, never plaintext
- **WHEN** any token is created
- **THEN** the `api_token.token_hash` column stores blake3 of the plaintext, the response returns the plaintext exactly once, and no subsequent endpoint (GET / list / show) ever returns the plaintext again

### Requirement: PAT permissions are live; API Token permissions are snapshot subset

The system SHALL load `Principal.permissions` for a token in one of two ways depending on `api_token.permissions`:

- When `permissions IS NULL` (PAT): the system SHALL load the owner user's current effective permissions by joining `user_role` → `role_permission` at request time.
- When `permissions IS NOT NULL` (API Token): the system SHALL use the stored JSONB array as the principal's permission set, with no runtime join.

The system SHALL reject API Token creation requests whose `permissions` list contains any permission not currently held by the creator.

#### Scenario: PAT reflects role revocation immediately
- **GIVEN** a user with role Admin has created a PAT
- **WHEN** the user's Admin role is revoked
- **AND** the PAT is used to call an endpoint requiring an Admin-only permission
- **THEN** the request returns `403 Forbidden` (no grace period)

#### Scenario: API Token retains its snapshot after creator role changes
- **GIVEN** a Release Manager created an API Token with `permissions=["release:publish", "artifact:upload"]`
- **WHEN** the creator's Release Manager role is revoked
- **AND** the API Token is used to call an endpoint requiring `release:publish`
- **THEN** the request still succeeds (snapshot is independent of creator's current roles)

#### Scenario: API Token creation rejects over-broad permissions
- **GIVEN** a user has permissions `{release:publish, artifact:upload}` (no `storage:manage`)
- **WHEN** the user POSTs to `/api/v1/tokens` with `{kind: "api", permissions: ["release:publish", "storage:manage"]}`
- **THEN** the server responds `422 Unprocessable Entity` with a problem+json body listing `storage:manage` as the over-broad permission and no row is inserted

### Requirement: Bearer authentication takes precedence over cookie session

The `Principal` extractor SHALL inspect the `Authorization` header first. When the header is present and matches `Bearer swhv_(pat|api)_<43>`, the extractor SHALL resolve via the Bearer path and SHALL NOT fall through to the cookie session path. When the header is absent or unparseable, the extractor SHALL fall through to the cookie session path.

#### Scenario: Authorization header with valid token bypasses cookie
- **GIVEN** a request carries both a valid session cookie and `Authorization: Bearer <valid PAT>`
- **WHEN** the request reaches a `Principal` extractor
- **THEN** the resolved Principal carries `auth_method = Pat { token_id }` (not Session)

#### Scenario: Malformed Authorization header is rejected without cookie fallback
- **WHEN** a request carries `Authorization: Bearer not-a-real-format`
- **THEN** the request returns `401 Unauthorized` with problem+json, regardless of any session cookie present

#### Scenario: Absent Authorization falls through to cookie
- **GIVEN** a request has no `Authorization` header and a valid session cookie
- **WHEN** the request reaches a `Principal` extractor
- **THEN** the resolved Principal carries `auth_method = Session { session_id }`

### Requirement: Revoked or expired tokens are rejected immediately

The system SHALL reject Bearer tokens whose `revoked_at IS NOT NULL` or whose `expires_at < now()`, returning `401 Unauthorized` with problem+json, before any business logic runs. Revocation SHALL take effect on the next request (no caching, no grace period).

#### Scenario: Revoked token is rejected on next use
- **GIVEN** a PAT has been used successfully
- **WHEN** the owner calls `DELETE /api/v1/tokens/:id` for that PAT
- **AND** the same plaintext token is used in a subsequent request
- **THEN** the subsequent request returns `401 Unauthorized` with problem+json

#### Scenario: Expired token is rejected
- **GIVEN** a token row has `expires_at` set to a past timestamp
- **WHEN** the plaintext token is used in a request
- **THEN** the request returns `401 Unauthorized` with problem+json

### Requirement: `last_used_at` is updated with 1-minute throttling

The system SHALL update `api_token.last_used_at` to `now()` on Bearer token use, but only when more than 1 minute has elapsed since the previous update (or when `last_used_at IS NULL`). The update SHALL be performed atomically via a single SQL `UPDATE ... WHERE id = $1 AND (last_used_at IS NULL OR last_used_at < now() - interval '1 minute')` statement.

#### Scenario: First use writes last_used_at
- **GIVEN** a token row has `last_used_at IS NULL`
- **WHEN** the token is used successfully
- **THEN** the row's `last_used_at` is updated to a timestamp within 5 seconds of the request and a `token_used_first_time` audit row is written

#### Scenario: Subsequent use within 1 minute does not write
- **GIVEN** a token was just used and `last_used_at` was updated 30 seconds ago
- **WHEN** the token is used again
- **THEN** the row's `last_used_at` is unchanged and no `token_used_first_time` audit row is added

#### Scenario: Use after 1 minute writes again
- **GIVEN** a token was last used more than 1 minute ago
- **WHEN** the token is used again
- **THEN** the row's `last_used_at` is updated to the new timestamp

### Requirement: CLI login endpoint is dedicated and rate-limited

The system SHALL expose `POST /api/v1/auth/cli-token` as a public endpoint that accepts `{email, password, token_name}` and returns `{token, name, kind: "pat", created_at}` upon successful password verification. The endpoint SHALL be rate-limited via `tower-governor` at 5 rps / burst 20 per source IP (same tier as `/api/v1/auth/login`).

#### Scenario: Correct credentials yield a PAT
- **GIVEN** a user `foo@example.com` exists with password `secret`
- **WHEN** the client POSTs `{email: "foo@example.com", password: "secret", token_name: "macbook-cli"}` to `/api/v1/auth/cli-token`
- **THEN** the response is `200 OK` with a JSON body containing a `swhv_pat_<...>` token and `name="macbook-cli"`, and a new `api_token` row exists with `kind='pat'` and `owner_user_id=<foo's id>`

#### Scenario: Wrong password is rejected with timing equality
- **GIVEN** a user `foo@example.com` exists
- **WHEN** the client POSTs an incorrect password
- **THEN** the response is `401 Unauthorized` with problem+json and no `api_token` row is created and a `token_created` audit row is NOT written

#### Scenario: Endpoint enforces governor rate limit
- **WHEN** more than 20 burst requests in under 1 second arrive from the same source IP
- **THEN** subsequent requests within the burst window receive `429 Too Many Requests`

### Requirement: Authenticated token management endpoints

The system SHALL expose three authenticated endpoints under `/api/v1/tokens`:

- `POST /api/v1/tokens` — requires `token:manage` permission. Creates a PAT or API Token (per request body `kind`). Returns the plaintext token exactly once.
- `GET /api/v1/tokens` — returns tokens owned by the authenticated user by default. Users with `token:manage` MAY list other users' tokens via `?owner=<user_id>`. The response SHALL never include plaintext tokens.
- `DELETE /api/v1/tokens/:id` — owners MAY revoke their own tokens; users with `token:manage` MAY revoke any token. Sets `revoked_at = now()`.

#### Scenario: Owner can list their own tokens without `token:manage`
- **GIVEN** a user has 2 PATs and lacks `token:manage`
- **WHEN** the user GETs `/api/v1/tokens` (no query string)
- **THEN** the response contains exactly 2 entries with `name`, `prefix`, `kind`, `created_at`, `last_used_at`, `expires_at`, `revoked_at`, and no `token` field

#### Scenario: Owner without `token:manage` cannot list others' tokens
- **GIVEN** the authenticated user lacks `token:manage`
- **WHEN** the user GETs `/api/v1/tokens?owner=<other-user-id>`
- **THEN** the response is `403 Forbidden` with problem+json

#### Scenario: Owner can revoke their own token without `token:manage`
- **GIVEN** the authenticated user owns a PAT with id `T`
- **WHEN** the user DELETEs `/api/v1/tokens/T`
- **THEN** the response is `204 No Content` and the row has `revoked_at` set and a `token_revoked` audit row is written

#### Scenario: Non-owner without `token:manage` cannot revoke
- **GIVEN** user A owns PAT `T` and user B is authenticated without `token:manage`
- **WHEN** user B DELETEs `/api/v1/tokens/T`
- **THEN** the response is `403 Forbidden` with problem+json and the row is unchanged

### Requirement: Audit log records token lifecycle events

The system SHALL write `audit_log` rows for the following token events:

- `token_created` — on successful POST to `/api/v1/tokens` or `/api/v1/auth/cli-token`. Fields: `actor_type` (user), `actor_id` (creator), `action`, `resource_type='api_token'`, `resource_id`, `ip`, `user_agent`, `metadata` containing `kind` and `name`.
- `token_revoked` — on successful DELETE `/api/v1/tokens/:id`. Same fields; `metadata` containing the revoked token's `id`, `kind`, and `name`.
- `token_used_first_time` — on the first request where Bearer resolution updates `last_used_at` from NULL. Fields: `actor_type=token`, `actor_id=token_id`, `metadata` containing the route path that triggered first use.

#### Scenario: token_created audit on cli-token success
- **WHEN** a CLI login succeeds
- **THEN** exactly one `audit_log` row exists with `action='token_created'` and `actor_type='user'`

#### Scenario: token_revoked audit on DELETE
- **WHEN** a user successfully DELETEs their token
- **THEN** exactly one `audit_log` row exists with `action='token_revoked'` and `actor_type='user'`

#### Scenario: token_used_first_time audit only fires once
- **GIVEN** a token's `last_used_at` is NULL
- **WHEN** the token is used 10 times in 30 seconds
- **THEN** exactly one `audit_log` row exists with `action='token_used_first_time'` and `actor_type='token'`

### Requirement: CLI `swarmhive login` writes credentials with 0600 permissions

The CLI command `swarmhive login [server]` SHALL prompt the user for email and password (interactively or via stdin), POST to `<server>/api/v1/auth/cli-token` with `token_name` defaulting to `<hostname>-<unix-timestamp>`, and on success write the returned token to `~/.config/swarmhive/credentials.toml` with file permissions `0600` (owner read/write only).

#### Scenario: Successful login writes a 0600 credentials file
- **GIVEN** the user provides a valid email and password
- **WHEN** `swarmhive login http://localhost:3030` returns successfully
- **THEN** the file `~/.config/swarmhive/credentials.toml` exists with mode `0600` and contains `server`, `token`, and `email` keys

#### Scenario: Failed login does not write credentials file
- **GIVEN** the user provides an invalid password
- **WHEN** `swarmhive login` exits with non-zero status
- **THEN** the credentials file is unchanged (not created or overwritten)

### Requirement: CLI prefers `SWARMHIVE_TOKEN` env over credentials file

CLI subcommands that require authentication SHALL resolve their bearer token in priority order: (1) `SWARMHIVE_TOKEN` environment variable, then (2) `token` field from `~/.config/swarmhive/credentials.toml`.

#### Scenario: Env overrides file
- **GIVEN** both `SWARMHIVE_TOKEN=swhv_pat_envvalue` is set and the credentials file contains a different token
- **WHEN** an authenticated CLI subcommand runs
- **THEN** the request is made with `Authorization: Bearer swhv_pat_envvalue`

#### Scenario: File used when env absent
- **GIVEN** `SWARMHIVE_TOKEN` is unset and the credentials file contains a valid token
- **WHEN** an authenticated CLI subcommand runs
- **THEN** the request is made with the file's token
