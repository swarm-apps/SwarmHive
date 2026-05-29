# mail-cli-admin Specification

## Purpose
TBD - created by archiving change add-cli-storage-mail-admin. Update Purpose after archive.
## Requirements
### Requirement: Mail HTTP DTOs SHALL be available in api-types

The mail HTTP DTOs (`MailProviderView`, `CreateProviderReq`, `UpdateProviderReq`, `MailTemplateView`, `UpdateTemplateReq`, `PreviewReq`, `PreviewResp`, `MailLogView`, `MailStatusResp`, plus the `ProviderKind` / `SmtpEncryption` / `MailLogStatus` enums) SHALL live in `swarmhive-api-types` so the CLI (which must not depend on entity/sea-orm) can consume them. The wire format SHALL be unchanged: enums serialize to the same lowercase strings (`smtp` / `starttls` / `tls` / `none` / `sent` / `failed`). The server SHALL consume these types from api-types; `From<&entity::Model>` conversions SHALL live in the entity crate.

#### Scenario: CLI deserializes a provider without entity deps

- **WHEN** the CLI lists mail providers
- **THEN** it deserializes `MailProviderView` from `swarmhive-api-types`
- **AND** `cargo tree -p swarmhive-cli` contains no `sea-orm`

#### Scenario: Wire format is unchanged after the DTO move

- **WHEN** a provider's `kind` / `encryption` are serialized
- **THEN** they remain the same lowercase strings as before the move (verified by a round-trip test and an unchanged `openapi_surface`)

### Requirement: CLI SHALL manage mail providers

The CLI SHALL provide `mail providers {list, create, update, activate, delete, test}`. `create` / `update` take SMTP fields (host, port, encryption, from-email, …); `delete --id --yes` revokes; `activate --id` switches the active provider; `test --id` sends a self-test. All honor the global `--output`.

#### Scenario: Create and activate an SMTP provider

- **WHEN** the user runs `swarmhive mail providers create --name prod --host smtp.example.com --port 587 --encryption starttls --from-email no-reply@example.com …` then `mail providers activate --id <id>`
- **THEN** the first POSTs `/api/v1/mail/providers` and the second POSTs `/activate`

#### Scenario: Delete requires --yes

- **WHEN** `swarmhive mail providers delete --id <id>` runs without `--yes`
- **THEN** the CLI refuses and exits non-zero; with `--yes` it DELETEs the provider

### Requirement: CLI SHALL accept the SMTP password without exposing it

`mail providers create` / `update` SHALL accept the SMTP `password` via, in precedence order, `--secret-stdin`, the `SWARMHIVE_MAIL_PASSWORD` env var, or a `--password <value>` flag; on `update`, omitting all three leaves the stored password unchanged. The same leak warning as the storage secret applies.

#### Scenario: Password via stdin

- **WHEN** the user runs `printf '%s' "$PW" | swarmhive mail providers create … --secret-stdin`
- **THEN** the password is read from stdin and never appears in argv or logs

### Requirement: CLI SHALL manage mail templates and read logs

The CLI SHALL provide `mail templates {list, get, set, preview, restore-defaults}`, `mail logs {list}`, and `mail status`. `templates set --id [--subject …] [--html-file <path>] [--text-file <path>]` updates only the provided fields (multi-line bodies read from files). `templates preview --id --sample-file <json>` returns the rendered subject/html/text. `restore-defaults` reseeds the built-in templates.

#### Scenario: Set a template body from files

- **WHEN** the user runs `swarmhive mail templates set --id <id> --subject "Welcome" --html-file invite.html --text-file invite.txt`
- **THEN** the PUT carries only the provided fields, with bodies read from the files
- **AND** omitted fields are left unchanged

#### Scenario: Preview a template with sample context

- **WHEN** the user runs `swarmhive mail templates preview --id <id> --sample-file ctx.json`
- **THEN** the rendered subject / html / text are returned (and printed as JSON under `--output json`)

