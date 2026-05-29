## MODIFIED Requirements

### Requirement: Server SHALL complete uploads idempotently, writing artifacts and optionally publishing

The server SHALL expose `POST /api/v1/apps/:slug/releases/:ver/uploads/:upload_id/complete` requiring `artifact:upload`, accepting `{ parts: [{ object_key, sha256, etag?, signature? }], publish?: bool }`. In one transaction it SHALL `HeadObject` each part to confirm checksum + size (no re-download), upsert the corresponding `artifact` rows (`ON CONFLICT` idempotent), and mark `upload_session=completed`. When a part carries a non-empty `signature`, the server SHALL persist it to that artifact's `signature_metadata` as `{ "tauri_signature": <signature> }`; when absent, `signature_metadata` is left unchanged (`null` on insert). When `publish=true` it SHALL additionally require `release:publish`, verify the release now has at least one artifact, set the release `published` with `published_at`, and write a publish `audit_log`. A mismatch SHALL return `422` `type=upload_checksum_mismatch` and write an audit row. Repeating complete with the same `upload_id` SHALL return the same `release_id`.

#### Scenario: Complete writes artifacts and publishes

- **GIVEN** an `upload_id` whose files were uploaded, and a principal holding `artifact:upload` + `release:publish`
- **WHEN** it POSTs complete with `publish: true`
- **THEN** the server HEADs each object and confirms checksum + size
- **AND** `artifact` rows are created for the release
- **AND** the release becomes `published` with `published_at` set and a publish audit row
- **AND** the response carries `release_id` and download `endpoints`

#### Scenario: Complete persists a Tauri signature

- **GIVEN** an `upload_id` for a Tauri bundle whose complete part carries a non-empty `signature`
- **WHEN** the client POSTs complete
- **THEN** the upserted `artifact` row's `signature_metadata` is `{ "tauri_signature": <signature> }`
- **AND** a part with no `signature` leaves `signature_metadata` unset

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

## ADDED Requirements

### Requirement: Server SHALL configure bucket CORS for browser direct upload

The server SHALL expose `POST /api/v1/storage/backends/:id/cors` requiring `storage:manage`, accepting `{ allowed_origins: [string] }`. It SHALL build the named backend's S3 client and call `PutBucketCors` with a rule allowing methods `PUT, GET, HEAD`, headers `*`, exposed header `ETag`, for the supplied origins. It SHALL return `{ ok: bool, detail: string }`: `ok=true` on success, `ok=false` with a human-readable `detail` (pointing to manual configuration) when the backend rejects `PutBucketCors` (e.g. Aliyun OSS S3-compatibility). The endpoint SHALL `404` when the backend id does not exist.

#### Scenario: CORS applied to an S3-compatible backend

- **GIVEN** a principal holding `storage:manage` and an existing backend on RustFS/MinIO/S3
- **WHEN** it POSTs cors with `{ allowed_origins: ["https://hive.example.com"] }`
- **THEN** the bucket CORS allows `PUT/GET/HEAD` from that origin with `ETag` exposed
- **AND** the response is `{ ok: true }`

#### Scenario: Backend rejecting PutBucketCors returns a manual-fallback result

- **GIVEN** a backend whose S3-compatibility layer does not support `PutBucketCors`
- **WHEN** the client POSTs cors
- **THEN** the response is `{ ok: false, detail: <manual configuration guidance> }` (not a 5xx)

#### Scenario: CORS without storage:manage is rejected

- **GIVEN** a principal lacking `storage:manage`
- **WHEN** it POSTs cors
- **THEN** the response is `403` `type=forbidden`
