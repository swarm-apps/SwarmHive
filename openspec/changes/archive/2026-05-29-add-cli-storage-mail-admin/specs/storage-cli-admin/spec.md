## ADDED Requirements

### Requirement: CLI SHALL manage storage backends

The CLI SHALL extend the `storage` command group with `{get, create, update, test, activate, cors}` against the storage endpoints. `create` / `update` take connection fields (endpoint, bucket, region, access-key-id, force-path-style, url-mode, …); `--backend <id|name>` selects an existing backend (name resolved via the list). All honor the global `--output`. There is no `delete` (single-active hot-swap model has no DELETE).

#### Scenario: Create then activate a backend

- **WHEN** the user runs `swarmhive storage create --name minio --endpoint http://… --bucket b --region us-east-1 --access-key-id k …` then `swarmhive storage activate --backend minio`
- **THEN** the first POSTs `/api/v1/storage/backends` and the second POSTs `/activate`
- **AND** `--output json` prints the resulting backend object

#### Scenario: Test and configure CORS

- **WHEN** the user runs `swarmhive storage test --backend minio` then `swarmhive storage cors --backend minio`
- **THEN** test POSTs `/test` and surfaces the probe result; cors POSTs `/cors` with the configured origin(s)

### Requirement: CLI SHALL accept the S3 secret without exposing it on the command line

`storage create` / `update` SHALL accept the `access_key_secret` via, in precedence order, `--secret-stdin` (piped), the `SWARMHIVE_STORAGE_SECRET` env var, or a `--access-key-secret <value>` flag. On `update`, omitting all three SHALL leave the stored secret unchanged. Documentation SHALL warn that the plaintext flag leaks to shell history / process list / CI logs and that AI / scripts SHOULD use env or `--secret-stdin`.

#### Scenario: Secret via env, not on the command line

- **GIVEN** `SWARMHIVE_STORAGE_SECRET` is set
- **WHEN** the user runs `swarmhive storage create …` without `--access-key-secret`
- **THEN** the secret is read from the env var and sent in the request body, never appearing in argv

#### Scenario: Update without a secret keeps the existing one

- **WHEN** `swarmhive storage update --backend minio --bucket newbucket` runs with no secret provided by any of the three inputs
- **THEN** the PATCH omits `access_key_secret` and the server retains the existing secret
