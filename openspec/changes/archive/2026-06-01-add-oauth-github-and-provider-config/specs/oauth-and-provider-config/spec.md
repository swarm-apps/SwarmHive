# oauth-and-provider-config

## ADDED Requirements

### Requirement: Server SHALL expose OAuth provider CRUD behind auth:manage permission

The server SHALL expose `GET /api/v1/auth/providers`, `POST /api/v1/auth/providers`, `PUT /api/v1/auth/providers/:id`, `DELETE /api/v1/auth/providers/:id`, and `POST /api/v1/auth/providers/:id/test`. All endpoints SHALL require the `auth:manage` permission. The `client_secret_encrypted` column SHALL be encrypted at rest using AES-256-GCM with `SWARMHIVE_SECRET_KEY` and SHALL never appear in any API response.

#### Scenario: Non-owner is blocked

- **GIVEN** a session for a `publisher` (no `auth:manage`)
- **WHEN** the client GETs `/api/v1/auth/providers`
- **THEN** the response is `403 Forbidden` with `type=missing_permission` and `required_permission: "auth:manage"`

#### Scenario: Secret never round-trips through GET

- **GIVEN** an `oauth_provider` row with a stored client_secret
- **WHEN** the client GETs `/api/v1/auth/providers`
- **THEN** the response items contain `client_id` but no `client_secret` and no `client_secret_encrypted`

### Requirement: Server SHALL expose a public list of enabled OAuth providers

The server SHALL expose `GET /api/v1/auth/oauth/providers` (no authentication required) returning the list of enabled providers as `[{ name, kind }]`. The Admin SPA `/login` page SHALL consume this list to render OAuth sign-in buttons.

#### Scenario: Public list returns only enabled providers

- **GIVEN** two `oauth_provider` rows, one with `enabled=true` and one with `enabled=false`
- **WHEN** an unauthenticated client GETs `/api/v1/auth/oauth/providers`
- **THEN** the response is `200 OK` with exactly one item (the enabled one)
- **AND** no `client_id` / `client_secret` fields are present in the response

### Requirement: Server SHALL implement OAuth start and callback for sign-in

The server SHALL implement `GET /api/v1/auth/oauth/:provider_name/start` (issues a redirect to the provider's `authorize_url` with PKCE + state stored in the session) and `GET /api/v1/auth/oauth/:provider_name/callback` (validates state, exchanges code, and either issues a session or returns a typed problem+json error). The callback SHALL return:

- `302` redirect with session cookie set when an existing `identity_link` matches
- `302` redirect to `/login?oauth_conflict=<provider_name>` when the OAuth email is verified but already bound to a password user with no `identity_link` for this provider
- `401` with `type=oauth_registration_disabled` when the OAuth email is unknown and self-register is not enabled (per ⑤ once landed)
- `422` with `type=oauth_no_verified_email` when the provider returns no verified email
- `400` with `type=oauth_state_mismatch` when session state does not match callback state

#### Scenario: Existing link signs the user in

- **GIVEN** an `identity_link { kind: github, subject: 42, user_id: U }` exists
- **WHEN** the OAuth callback resolves to subject 42
- **THEN** a session cookie is set for user U
- **AND** the response is a `302` redirect to the `next` path stored in the session

#### Scenario: Email conflict produces friendly redirect

- **GIVEN** a password user with `email='a@x.com'` exists and no `identity_link` for github subject 99
- **WHEN** the OAuth callback returns subject 99 with verified email `a@x.com`
- **THEN** the response is a `302` redirect to `/login?oauth_conflict=github` (no email in URL)
- **AND** no `identity_link` row is created

### Requirement: Server SHALL block OAuth sign-in during bootstrap

While `user` table is empty (bootstrap window active), `GET /api/v1/auth/oauth/:provider_name/start` SHALL return `410 Gone` with `type=oauth_not_available_during_bootstrap` regardless of provider configuration. The first Owner SHALL be created exclusively via `POST /api/v1/setup` with email and password.

#### Scenario: Empty DB rejects OAuth start

- **GIVEN** the `user` table is empty and an enabled `oauth_provider` row exists
- **WHEN** an unauthenticated client GETs `/api/v1/auth/oauth/github/start`
- **THEN** the response is `410 Gone` with `type=oauth_not_available_during_bootstrap`

### Requirement: Server SHALL support OAuth link and unlink from authenticated sessions

The server SHALL expose `GET /api/v1/auth/oauth/providers/link/:provider_name/start` (authenticated; reuses the callback handler with mode='link') and `DELETE /api/v1/auth/oauth/links/:provider_name` (authenticated; removes the `identity_link`). (Link-start is a GET, not a POST: it is a top-level browser navigation that must redirect cross-origin to the provider — the actual link is created only in the post-approval callback, so a GET carries no CSRF risk.) Unlink SHALL be rejected with `409` `type=cannot_unlink_only_auth_method` when the user has no `user_credentials` row.

#### Scenario: Unlink fails for OAuth-only user

- **GIVEN** a user U with no `user_credentials` row and one `identity_link` for github
- **WHEN** U calls `DELETE /api/v1/auth/oauth/links/github`
- **THEN** the response is `409` with `type=cannot_unlink_only_auth_method`
- **AND** the `identity_link` row remains

#### Scenario: Link is idempotent for the same user

- **GIVEN** user U is already linked to github subject 99
- **WHEN** U starts the link flow again and the callback returns subject 99
- **THEN** no duplicate `identity_link` is created
- **AND** the response is `302` redirect to `/profile`

### Requirement: Admin SPA SHALL render Settings > Authentication for OAuth provider config

The Admin SPA SHALL add a `Settings > Authentication` sub-page implementing CRUD for `oauth_provider` via ProTable + ProDrawerForm + Test action. The page SHALL be permission-gated on `auth:manage`. Creating a provider with `kind=Github` SHALL auto-prefill default `authorize_url`, `token_url`, `userinfo_url`, and `scopes`.

#### Scenario: GitHub kind auto-prefills URLs

- **GIVEN** the user opens the new-provider drawer in `/settings/authentication`
- **WHEN** the user selects kind `GitHub`
- **THEN** the `authorize_url`, `token_url`, `userinfo_url` fields are pre-filled with the GitHub public URLs
- **AND** the `scopes` multi-select is pre-filled with `["read:user", "user:email"]`

### Requirement: Admin SPA SHALL render OAuth sign-in buttons on /login

The Admin SPA `/login` route SHALL query `GET /api/v1/auth/oauth/providers` and render one button per enabled provider. Clicking a button SHALL navigate the browser to `/api/v1/auth/oauth/<name>/start?next=<encoded next>`. When the public list is empty, the OAuth button section SHALL be hidden entirely (no empty divider, no placeholder).

#### Scenario: OAuth section hidden when no providers enabled

- **GIVEN** the public provider list is empty
- **WHEN** the user opens `/login`
- **THEN** the rendered DOM does not contain any OAuth sign-in button or visible divider for the OAuth section

#### Scenario: oauth_conflict search param renders alert

- **GIVEN** the user is redirected to `/login?oauth_conflict=github`
- **THEN** the page displays a localized Alert instructing the user to sign in with their existing password and then link GitHub from their Profile
- **AND** the Alert does not contain any email address (privacy-conscious wording)

### Requirement: Admin SPA SHALL render Profile > Linked accounts

The Admin SPA SHALL add a `/profile` route showing the current user's `identity_link` rows. Each row SHALL have an Unlink action that confirms via Modal. The Unlink action SHALL be disabled for users with no `user_credentials` (OAuth-only accounts).

#### Scenario: Unlink disabled for OAuth-only user

- **GIVEN** the current user has no password credentials and one GitHub link
- **WHEN** the user visits `/profile`
- **THEN** the Unlink button on the GitHub row is disabled
- **AND** a tooltip explains "Set a password first to unlink your only sign-in method"
