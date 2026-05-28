# mail-infrastructure Specification

## Purpose
TBD - created by archiving change add-mail-infrastructure. Update Purpose after archive.
## Requirements
### Requirement: Server SHALL expose mail provider CRUD endpoints behind mail:manage permission

The server SHALL expose `GET /api/v1/mail/providers`, `POST /api/v1/mail/providers`, `PUT /api/v1/mail/providers/:id`, `DELETE /api/v1/mail/providers/:id`, `POST /api/v1/mail/providers/:id/test`, and `POST /api/v1/mail/providers/:id/activate`. All endpoints SHALL require the `mail:manage` permission and reject other principals with `403` problem+json. At most one `mail_provider` row SHALL have `active=true` at any time (enforced by partial unique DB index).

#### Scenario: Non-owner is blocked from CRUD

- **GIVEN** a session belongs to a `publisher` (no `mail:manage`)
- **WHEN** the client calls `GET /api/v1/mail/providers`
- **THEN** the response is `403 Forbidden` with `application/problem+json` `type=missing_permission` and `required_permission: "mail:manage"`

#### Scenario: Activating a provider deactivates others

- **GIVEN** two `mail_provider` rows exist with provider A `active=true`
- **WHEN** the client POSTs `/api/v1/mail/providers/B/activate`
- **THEN** the response is `200 OK`
- **AND** querying providers shows A `active=false` and B `active=true`

### Requirement: Server SHALL store SMTP passwords encrypted at rest

The server SHALL encrypt the SMTP `password` field of every `mail_provider` row using AES-256-GCM with a key derived from the `SWARMHIVE_MAIL_PASSWORD_KEY` environment variable. The `password_encrypted` column SHALL never be returned to API clients in any response. Provider `GET` responses SHALL include `password_set: bool` instead. The server SHALL fail to start when `SWARMHIVE_MAIL_PASSWORD_KEY` is unset.

#### Scenario: GET never returns plaintext

- **WHEN** the client GETs `/api/v1/mail/providers/:id`
- **THEN** the response body does not contain the plaintext password
- **AND** the body contains `password_set: true` when a password has been configured

#### Scenario: Missing key fails startup

- **GIVEN** the server is launched without `SWARMHIVE_MAIL_PASSWORD_KEY`
- **WHEN** the binary starts
- **THEN** it exits with a non-zero status and a log line referencing the missing key

### Requirement: Server SHALL render templates via minijinja with safe error reporting

The server SHALL expose `GET /api/v1/mail/templates`, `PUT /api/v1/mail/templates/:id`, and `POST /api/v1/mail/templates/:id/preview`. Template rendering SHALL use minijinja. A template with invalid syntax SHALL cause `/preview` to return `422 Unprocessable Entity` with `application/problem+json` `type=template_invalid` carrying the parse error message, and SHALL NOT crash the server. On first startup, the server SHALL seed default templates for `password_reset`, `user_invite`, `email_verify`, `security_alert` in both `en` and `zh-CN`.

#### Scenario: Invalid template is rejected gracefully

- **GIVEN** an existing template with body `Hello {{ name`
- **WHEN** the client POSTs `/api/v1/mail/templates/:id/preview` with sample data
- **THEN** the response is `422` with `type=template_invalid` and `detail` containing the minijinja parse error
- **AND** the server process keeps running and serves subsequent requests normally

#### Scenario: First boot seeds defaults

- **GIVEN** an empty `mail_template` table at server startup
- **WHEN** the server completes startup
- **THEN** the table contains exactly 8 rows: 4 events × 2 locales (`en`, `zh-CN`)

### Requirement: Server SHALL fall back to ConsoleMailer when no provider is active

When no `mail_provider` row has `active=true`, the server SHALL construct a `ConsoleMailer` that writes each outbound `MailEnvelope` to stdout and inserts a `mail_log` row with `status='sent'` and `provider_id=null`. The Admin SPA SHALL display a top banner reading "Mail not configured" while this fallback is active and the runtime profile is not `dev`.

#### Scenario: ConsoleMailer logs to stdout and DB

- **GIVEN** no active provider
- **WHEN** any caller invokes `Mailer::send(envelope)`
- **THEN** the rendered subject and bodies are written to stdout
- **AND** a row is inserted into `mail_log` with `status='sent'` and `provider_id IS NULL`

### Requirement: Admin SPA SHALL render Settings > Mail with provider / template / logs sub-pages

The Admin SPA SHALL add a `Settings > Mail` menu group with three sub-pages: `Providers` (ProTable + ProDrawerForm CRUD), `Templates` (Monaco editor + preview), and `Logs` (paginated ProTable with error inspection). All sub-pages SHALL inherit the `_auth` guard and the page-level `mail:manage` permission gate.

#### Scenario: Provider list renders with active highlight

- **GIVEN** the user has `mail:manage` and visits `/settings/mail`
- **THEN** a ProTable lists all `mail_provider` rows
- **AND** the row with `active=true` is visually highlighted (e.g., Tag "Active")
- **AND** an "Activate" action is available on each inactive row

#### Scenario: Template preview round-trips through server

- **GIVEN** the user edits a template body in the Monaco editor
- **WHEN** the user clicks "Preview"
- **THEN** the SPA POSTs to `/api/v1/mail/templates/:id/preview` with sample data
- **AND** displays the rendered HTML inside an `<iframe srcDoc>` (isolated from the main DOM)

#### Scenario: Logs page surfaces failures

- **GIVEN** a row in `mail_log` with `status='failed'` and a non-null `error` field
- **WHEN** the user expands that row in `/settings/mail/logs`
- **THEN** the expand panel shows the full `error` text

