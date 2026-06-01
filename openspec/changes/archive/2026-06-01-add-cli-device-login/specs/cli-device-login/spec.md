# cli-device-login

## ADDED Requirements

### Requirement: Server SHALL issue device + user codes via the device-code endpoint

The server SHALL expose `POST /api/v1/auth/device/code` (no authentication). It SHALL generate a high-entropy `device_code` (stored only as a blake3 hash), a human-typable `user_code` (8 characters, RFC 8628 base-20 alphabet, formatted `XXXX-XXXX`), and persist a `device_authorization` row with `status=pending` and a 15-minute `expires_at`. The response SHALL include `device_code`, `user_code`, `verification_uri` (`{base_url}/device`), `verification_uri_complete` (`{base_url}/device?user_code=<code>`), `expires_in`, and `interval`.

#### Scenario: Device code request returns codes and URIs

- **GIVEN** at least one user exists (bootstrap window closed)
- **WHEN** a client POSTs `/api/v1/auth/device/code` with `{ client_id: "swarmhive-cli", token_name: "macbook-123" }`
- **THEN** the response is `200 OK` with a `user_code` matching `^[BCDFGHJKLMNPQRSTVWXZ]{4}-[BCDFGHJKLMNPQRSTVWXZ]{4}$`
- **AND** `verification_uri_complete` ends with `/device?user_code=<the user_code>`
- **AND** a `device_authorization` row exists with `status=pending` and `expires_at` ~15 minutes ahead
- **AND** the plaintext `device_code` does not appear in the database (only its blake3 hash)

#### Scenario: Device code is blocked during bootstrap window

- **GIVEN** the `user` table is empty (bootstrap window active)
- **WHEN** a client POSTs `/api/v1/auth/device/code`
- **THEN** the response is `410 Gone` with `type=device_not_available_during_bootstrap` (RFC 9457 problem+json)
- **AND** no `device_authorization` row is created

### Requirement: Server SHALL implement RFC 8628 token polling with OAuth-style errors

The server SHALL expose `POST /api/v1/auth/device/token` (no authentication) accepting `{ grant_type, device_code, client_id }` where `grant_type` is `urn:ietf:params:oauth:grant-type:device_code`. On success it SHALL mint a PAT (reusing the token service, `kind=pat`, `permissions=null`) attributed to the approving user, mark the row `status=completed`, and return `200` with `{ token, name, kind, created_at }`. All non-success outcomes SHALL use the RFC 8628 wire format `400 { "error": <code> }` (NOT RFC 9457) with codes `authorization_pending`, `slow_down`, `access_denied`, `expired_token`, `invalid_grant`, `unsupported_grant_type`, or `invalid_request`.

The state machine SHALL look the row up by `blake3(device_code)` and return `invalid_grant` when **no row is found** (unknown `device_code`, or a row already physically removed by lazy cleanup) — this not-found branch SHALL be evaluated before any status branch (a `completed` row must not be relied on to cover not-found). A `grant_type` other than the device-code URN SHALL return `unsupported_grant_type`; a missing `client_id` or one not equal to `swarmhive-cli` SHALL return `invalid_request`. The approved→completed transition SHALL be atomic (a conditional update gated on `status=approved`), so concurrent polls on one approved grant mint **at most one** PAT and write **at most one** `auth:token_created` audit row; a poll that loses the race SHALL receive `invalid_grant`.

#### Scenario: Pending authorization keeps the client polling

- **GIVEN** a `device_authorization` row with `status=pending` and not expired
- **WHEN** the client POSTs `/api/v1/auth/device/token` with the matching `device_code`
- **THEN** the response is `400` with body `{ "error": "authorization_pending" }`

#### Scenario: Polling faster than the interval is throttled

- **GIVEN** a pending row whose `last_polled_at` is more recent than `interval_secs` ago
- **WHEN** the client polls again
- **THEN** the response is `400` with body `{ "error": "slow_down" }`

#### Scenario: Approved authorization mints a PAT exactly once

- **GIVEN** a `device_authorization` row with `status=approved` and `user_id=U`
- **WHEN** the client polls `/api/v1/auth/device/token`
- **THEN** the response is `200` with a plaintext `token` of format `swhv_pat_<43>` and `kind=pat`
- **AND** the row transitions to `status=completed`
- **AND** an `auth:token_created` audit row is written attributed to user U
- **AND** a subsequent poll with the same `device_code` returns `400 { "error": "invalid_grant" }`

#### Scenario: Denied authorization stops the client

- **GIVEN** a `device_authorization` row with `status=denied`
- **WHEN** the client polls `/api/v1/auth/device/token`
- **THEN** the response is `400` with body `{ "error": "access_denied" }`

#### Scenario: Expired device code is rejected

- **GIVEN** a `device_authorization` row whose `expires_at` is in the past (and not yet lazily removed)
- **WHEN** the client polls `/api/v1/auth/device/token`
- **THEN** the response is `400` with body `{ "error": "expired_token" }`

#### Scenario: Unknown device code is rejected with invalid_grant

- **GIVEN** no `device_authorization` row matches `blake3(device_code)` (never existed, or already lazily removed)
- **WHEN** the client polls `/api/v1/auth/device/token`
- **THEN** the response is `400` with body `{ "error": "invalid_grant" }`
- **AND** the not-found branch is evaluated before any status branch

#### Scenario: First poll (before any prior poll) is not throttled

- **GIVEN** a pending row whose `last_polled_at` is null (never polled)
- **WHEN** the client polls `/api/v1/auth/device/token` for the first time
- **THEN** the response is `400 { "error": "authorization_pending" }` (NOT `slow_down`)
- **AND** `last_polled_at` is set to now
- **AND** a subsequent `slow_down` rejection does not refresh `last_polled_at`

#### Scenario: Concurrent polls on one approved grant mint exactly one PAT

- **GIVEN** a `device_authorization` row with `status=approved`
- **WHEN** two polls with the same `device_code` arrive concurrently
- **THEN** exactly one `api_token` row and exactly one `auth:token_created` audit row are created
- **AND** the losing poll receives `400 { "error": "invalid_grant" }`

#### Scenario: Wrong grant_type or client_id is rejected

- **GIVEN** a valid pending `device_code`
- **WHEN** the client polls with `grant_type` not equal to the device-code URN
- **THEN** the response is `400 { "error": "unsupported_grant_type" }`
- **AND** when `client_id` is missing or not `swarmhive-cli`, the response is `400 { "error": "invalid_request" }`

### Requirement: Server SHALL let authenticated users look up, approve, and deny device grants

The server SHALL expose `GET /api/v1/auth/device/lookup?user_code=<code>`, `POST /api/v1/auth/device/approve`, and `POST /api/v1/auth/device/deny`. All three SHALL require a Principal derived from a **browser session cookie**, and SHALL reject a `Bearer` token (PAT / API Token) with `403` — approval must be an interactive session so a PAT holder cannot self-approve a grant out of band. `lookup` SHALL return a `DeviceAuthorizationView` (no secrets — `user_code`, `client_id`, `client_name`, timestamps) or `404` when the code is unknown or expired. `approve` SHALL set `status=approved` and `user_id` to the current user; `deny` SHALL set `status=denied`. Both SHALL write an audit row (`auth:device_authorized` / `auth:device_denied`).

> Coordination with `add-registration-policy-and-self-register` (⑤): once that change introduces a `pending_approval` user status, `approve`/`deny` SHALL additionally require the approver's `user.status` to be `active` (returning `403` otherwise), mirroring the `_auth` guard's `pending_approval` interception. This constraint is reserved here and enforced when ⑤ lands; standalone this change has no `pending_approval` status to gate on.

#### Scenario: Lookup surfaces the requesting client to the approver

- **GIVEN** a pending `device_authorization` with `client_name="swarmhive @ macbook"` and `user_code="WDJB-MJHT"`
- **AND** an authenticated session
- **WHEN** the user GETs `/api/v1/auth/device/lookup?user_code=WDJB-MJHT`
- **THEN** the response is `200` containing `client_name` and `expires_at`
- **AND** the response contains no `device_code` or `device_code_hash`

#### Scenario: Approve records the user and is reflected in the next poll

- **GIVEN** an authenticated user U and a pending `user_code="WDJB-MJHT"`
- **WHEN** U POSTs `/api/v1/auth/device/approve` with `{ "user_code": "WDJB-MJHT" }`
- **THEN** the response is `204`
- **AND** the row has `status=approved` and `user_id=U`
- **AND** an `auth:device_authorized` audit row is written

#### Scenario: Lookup on an unknown code is not enumerable

- **GIVEN** no `device_authorization` row for `user_code="ZZZZ-ZZZZ"`
- **WHEN** an authenticated user GETs `/api/v1/auth/device/lookup?user_code=ZZZZ-ZZZZ`
- **THEN** the response is `404` (same shape as an expired code, no distinguishing detail)

#### Scenario: A Bearer PAT cannot approve a device grant

- **GIVEN** a pending `user_code` and a caller presenting `Authorization: Bearer swhv_pat_…` (no session cookie)
- **WHEN** the caller POSTs `/api/v1/auth/device/approve` with `{ "user_code": ... }`
- **THEN** the response is `403` (approval requires a browser session, not a token)
- **AND** the grant remains `pending`

### Requirement: Server SHALL remove the ROPC cli-token endpoint

The server SHALL no longer expose `POST /api/v1/auth/cli-token`. The `CliTokenRequest` and `CliTokenResponse` DTOs SHALL be removed from `swarmhive-api-types`. The endpoint SHALL be unmounted from both `build_router` and `openapi_router` so the generated OpenAPI document no longer advertises it.

#### Scenario: cli-token endpoint is gone

- **WHEN** a client POSTs `/api/v1/auth/cli-token` with any body
- **THEN** the response is `404 Not Found`
- **AND** the generated `/api/openapi.json` contains no `cli-token` path

### Requirement: CLI login SHALL use the device flow and never collect a password

The `swarmhive login [server]` command SHALL request a device code, print the `user_code` and `verification_uri`, attempt to open `verification_uri_complete` in the default browser, then poll the token endpoint respecting `interval` and `slow_down`. On success it SHALL fetch the user identity via `GET /api/v1/auth/me` and persist `{ server, email, token }` to `credentials.toml`. **Token acquisition is the success boundary**: once a PAT is minted, a failure of the subsequent `GET /api/v1/auth/me` SHALL NOT discard the token — the CLI SHALL still persist `{ server, token }` (with `email` blank/optional) and emit a warning, so a live PAT is never orphaned server-side. The command SHALL NOT prompt for a password and SHALL NOT send a password to the server. The `--email` flag SHALL be removed.

#### Scenario: Login completes without a password prompt

- **GIVEN** a running server with at least one user
- **WHEN** the user runs `swarmhive login http://localhost:3030`
- **THEN** the CLI prints a `user_code` and a verification URL
- **AND** the CLI never reads a password from the TTY
- **AND** after the grant is approved in the browser, `credentials.toml` contains a `swhv_pat_` token and the resolved email

#### Scenario: Denied or expired grant exits non-zero

- **GIVEN** the CLI is polling a device grant
- **WHEN** the grant is denied in the browser (or the code expires)
- **THEN** the CLI stops polling and exits with a non-zero status and a clear message

#### Scenario: /me failure after minting still persists the token

- **GIVEN** a PAT has just been minted and returned to the CLI
- **WHEN** the follow-up `GET /api/v1/auth/me` fails (network / 5xx)
- **THEN** the CLI still writes `{ server, token }` to `credentials.toml` (email left blank)
- **AND** it warns about the missing identity rather than discarding the token

#### Scenario: CLI keeps zero ORM dependency

- **WHEN** `cargo tree -p swarmhive-cli` is inspected
- **THEN** there is no `sea-orm` (or entity-crate) dependency in the CLI's tree

### Requirement: Admin SPA SHALL serve a public device-approval page that preserves the user_code across login

The Admin SPA SHALL add a top-level public route `/device` (not under the `_auth` guard, whose redirect would drop the `user_code` search param). When the visitor is unauthenticated, the page SHALL render a sign-in call-to-action linking to `/login?next=<encoded /device?user_code=...>` so the code survives the round-trip and the user authenticates with whatever methods `/login` offers (password, and GitHub once `add-oauth-github-and-provider-config` lands). When authenticated, the page SHALL pre-fill `user_code` from the search param, call `lookup` to display the requesting client, and offer Approve / Deny actions.

#### Scenario: Unauthenticated visit preserves the user_code through login

- **GIVEN** the visitor is not logged in
- **WHEN** they open `/device?user_code=WDJB-MJHT`
- **THEN** the page shows a sign-in prompt linking to `/login` with a `next` that, once decoded, equals `/device?user_code=WDJB-MJHT`
- **AND** after logging in they return to `/device` with `user_code=WDJB-MJHT` still present

#### Scenario: Authenticated visit shows the requesting client and approves

- **GIVEN** the visitor is logged in and a pending grant exists for `user_code=WDJB-MJHT`
- **WHEN** they open `/device?user_code=WDJB-MJHT`
- **THEN** the page displays the requesting `client_name`
- **AND** an Approve action POSTs to `/api/v1/auth/device/approve` and a Deny action POSTs to `/api/v1/auth/device/deny`
