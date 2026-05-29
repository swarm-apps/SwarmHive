# storage-and-presign-upload

## ADDED Requirements

### Requirement: Server SHALL manage S3-compatible storage backends with a single active backend

The server SHALL expose `GET/POST /api/v1/storage/backends`, `PATCH /api/v1/storage/backends/:id`, `POST /api/v1/storage/backends/:id/test`, and `POST /api/v1/storage/backends/:id/activate`, all requiring `storage:manage`. `access_key_secret` SHALL be stored encrypted via `crypto::SecretKey` and never returned (GET returns `secret_set: bool`). At most one backend SHALL be `active`; `activate` SHALL set all other backends inactive in the same transaction (no partial unique index). Activating or patching the active backend SHALL hot-swap the in-memory storage handle without a restart. `/test` SHALL perform a real `put`/`get`/`delete` of a `.swarmhive-probe` object and detect whether the backend supports `x-amz-checksum-sha256`, recording the result on the backend row.

#### Scenario: Activating a backend deactivates others and hot-swaps

- **GIVEN** two storage backends A (active) and B (inactive)
- **WHEN** a principal holding `storage:manage` POSTs `/api/v1/storage/backends/<B>/activate`
- **THEN** B becomes `active=true` and A becomes `active=false`
- **AND** subsequent presign requests use B without a server restart

#### Scenario: Secret is never returned

- **WHEN** a principal GETs `/api/v1/storage/backends`
- **THEN** each backend includes `secret_set: bool` and omits `access_key_secret`/its ciphertext

#### Scenario: Test performs a real probe round-trip

- **GIVEN** a configured backend
- **WHEN** a principal POSTs `/api/v1/storage/backends/:id/test`
- **THEN** the server puts, gets, and deletes a `.swarmhive-probe` object
- **AND** records `supports_sha256_checksum` based on whether the backend honored the checksum

### Requirement: Server SHALL issue per-file presigned PUT URLs bound to expected SHA256

The server SHALL expose `POST /api/v1/apps/:slug/releases/:ver/uploads/presign` requiring `artifact:upload`, accepting `{ files: [{ relative_path, size, expected_sha256, platform, target?, arch?, abi? }] }`. For each file it SHALL compute an `object_key` of the form `{prefix}/apps/{slug}/versions/{version}/{platform}/{target}/{filename}` (no channel segment) and return a presigned PUT URL that binds `x-amz-checksum-sha256` to the supplied `expected_sha256`, with a 5–10 minute expiry. The target release SHALL already exist; presign SHALL NOT create it. When no storage backend is active the endpoint SHALL return `409` `type=storage_not_configured`.

#### Scenario: Presign returns one part per file with checksum binding

- **GIVEN** an active storage backend and an existing draft release `0.4.5`
- **WHEN** a principal holding `artifact:upload` POSTs presign with two files
- **THEN** the response contains `upload_id` and a `parts` array of length 2
- **AND** each part has an `object_key` without a `channels/` segment
- **AND** each presigned URL binds `x-amz-checksum-sha256` to that file's `expected_sha256`

#### Scenario: Presign without an active backend is rejected

- **GIVEN** no active storage backend
- **WHEN** a principal POSTs a presign request
- **THEN** the response is `409` `type=storage_not_configured`

#### Scenario: Tampered bytes are rejected by object storage

- **GIVEN** a presigned URL bound to a file's `expected_sha256`
- **WHEN** the client PUTs bytes whose sha256 differs from `expected_sha256`
- **THEN** object storage rejects the PUT with a 4xx (checksum mismatch), so no corrupt object is stored

### Requirement: Server SHALL complete uploads idempotently, writing artifacts and optionally publishing

The server SHALL expose `POST /api/v1/apps/:slug/releases/:ver/uploads/:upload_id/complete` requiring `artifact:upload`, accepting `{ parts: [{ object_key, sha256, etag }], publish?: bool }`. In one transaction it SHALL `HeadObject` each part to confirm checksum + size (no re-download), upsert the corresponding `artifact` rows (`ON CONFLICT` idempotent), and mark `upload_session=completed`. When `publish=true` it SHALL additionally require `release:publish`, verify the release now has at least one artifact, set the release `published` with `published_at`, and write a publish `audit_log`. A mismatch SHALL return `422` `type=upload_checksum_mismatch` and write an audit row. Repeating complete with the same `upload_id` SHALL return the same `release_id`.

#### Scenario: Complete writes artifacts and publishes

- **GIVEN** an `upload_id` whose files were uploaded, and a principal holding `artifact:upload` + `release:publish`
- **WHEN** it POSTs complete with `publish: true`
- **THEN** the server HEADs each object and confirms checksum + size
- **AND** `artifact` rows are created for the release
- **AND** the release becomes `published` with `published_at` set and a publish audit row
- **AND** the response carries `release_id` and download `endpoints`

#### Scenario: Complete is idempotent

- **GIVEN** a completed `upload_id`
- **WHEN** the client POSTs complete again with the same `upload_id`
- **THEN** the response returns the same `release_id`
- **AND** no duplicate `artifact` rows are created

#### Scenario: publish=true without release:publish is rejected

- **GIVEN** a principal holding `artifact:upload` but not `release:publish` (the `developer` role)
- **WHEN** it POSTs complete with `publish: true`
- **THEN** the response is `403` `type=forbidden` carrying `required_permission: "release:publish"`
- **AND** the release remains `draft` (not silently left in an ambiguous state)

#### Scenario: Checksum mismatch at complete is rejected and audited

- **GIVEN** an `upload_id` where a reported `sha256` does not match the stored object
- **WHEN** the client POSTs complete
- **THEN** the response is `422` `type=upload_checksum_mismatch`
- **AND** an `audit_log` row records the failure
- **AND** no `artifact` row is written for the mismatched file

### Requirement: Server SHALL serve downloads via redirect and record intent

The server SHALL expose `GET /download/:app/:version/:artifact_id` (public or read-only app key). It SHALL record a `download_intent`, then return `302` to a `public` or `signed` URL per the active backend's `url_mode`, without proxying bytes. Artifacts of a `yanked` release SHALL return `404`.

#### Scenario: Download redirects to a storage URL

- **GIVEN** a published release with an artifact
- **WHEN** a client GETs `/download/swarmdrop/0.4.5/<artifact_id>`
- **THEN** a `download_intent` is recorded
- **AND** the response is `302` to a public or signed object URL (no byte proxying)

#### Scenario: Download of a yanked release is gone

- **GIVEN** a release that has been yanked
- **WHEN** a client GETs `/download/...` for one of its artifacts
- **THEN** the response is `404`

### Requirement: CLI SHALL verify and publish artifacts via presign + complete

The `swarmhive` CLI SHALL provide `verify tauri|android` and `publish tauri|android`. `verify` SHALL check artifact existence, parse `latest.json` (Tauri), compute sha256, and query the server for a duplicate version; it SHALL trust `--version`/`--version-code` flags rather than parsing APK binaries or `build.gradle`. `publish` SHALL read `swarmhive.toml` (single app, `--app` override; Tauri version auto-read from `tauri.conf.json`, Android version via explicit flags), ensure a draft release exists, presign, stream-upload each file with a progress bar sending `x-amz-checksum-sha256`, retry transient failures per file, and call complete (default `publish=true`). The HTTP client SHALL use rustls with system root certs and honor `--ca-cert`/`SWARMHIVE_CA_CERT`.

#### Scenario: Publish uploads with progress and resumes failed files

- **GIVEN** a logged-in CLI, an active server backend, and a Tauri bundle
- **WHEN** the user runs `swarmhive publish tauri --app swarmdrop --channel stable`
- **THEN** the CLI presigns, uploads each file with a progress bar, and calls complete
- **AND** a transient failure on one file retries only that file (not the already-uploaded ones)
- **AND** on success the release is published and download endpoints are printed

#### Scenario: Verify trusts version flags without parsing binaries

- **WHEN** the user runs `swarmhive verify android --app swarmnote-rn --version 0.2.1 --version-code 21 --apk <path>`
- **THEN** the CLI confirms the APK file exists and computes its sha256
- **AND** it uses the supplied `--version`/`--version-code` without parsing the APK's binary manifest
- **AND** it warns if the server already has that version published
