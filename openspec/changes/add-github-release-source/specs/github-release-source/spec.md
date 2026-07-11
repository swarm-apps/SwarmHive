## ADDED Requirements

### Requirement: Server SHALL manage a per-app GitHub Release source

The server SHALL expose CRUD for a single GitHub Release download source per app, requiring `app:update`. The config SHALL hold `owner`, `repo`, `tag_template` (default `v{version}`), `enabled`, and an optional access token. The access token SHALL be encrypted at rest via the server secret key and MUST NEVER be returned by any API; the view SHALL expose only `token_set: bool`. At most one GitHub source SHALL exist per app, enforced by a full unique constraint on `app_id` (NOT a partial index). The token SHALL be used ONLY for server-side liveness/digest probing and rate-limit relief — it SHALL NOT be used to deliver bytes to clients.

#### Scenario: Create and read a GitHub source hides the token

- **WHEN** a principal holding `app:update` creates a GitHub source with `owner`, `repo`, and an access token
- **THEN** the source is persisted with `enabled` and `tag_template`
- **AND** any read returns `token_set: true` and never the token value

#### Scenario: Second GitHub source per app is rejected

- **GIVEN** an app that already has a GitHub source
- **WHEN** a second GitHub source is created for the same app
- **THEN** the request is rejected by the unique constraint (not silently overwritten)

#### Scenario: Blank token on update keeps the stored token

- **GIVEN** a GitHub source with a stored token
- **WHEN** the source is updated with an omitted/blank token field
- **THEN** the stored token is preserved unchanged

### Requirement: Server SHALL record a verbatim GitHub asset URL per artifact and validate it at store time

When an artifact is created or updated, the server SHALL accept an optional `mirror_url` (the exact GitHub Release asset URL, as computed by CI) and persist it verbatim on the artifact — it SHALL NOT reconstruct the URL from `artifact.filename`, because CI renames the GitHub asset. At store time the server SHALL validate that a supplied `mirror_url` targets the `github.com` host AND the `owner/repo` of the app's configured GitHub source; a URL failing either check SHALL be rejected. On re-upload of the same artifact identity, `mirror_url` SHALL be refreshed together with the bytes, and SHALL be cleared when the new upload carries no `mirror_url` (it MUST NOT retain a stale URL pointing at superseded bytes).

#### Scenario: Renamed GitHub asset URL is stored and served verbatim

- **GIVEN** an app with a GitHub source `owner/repo`
- **WHEN** an artifact is recorded with `mirror_url = https://github.com/owner/repo/releases/download/v0.7.15/mobile-v0.7.15-app-release.apk`
- **THEN** the URL is persisted exactly as given
- **AND** it is later served without being reconstructed from `artifact.filename`

#### Scenario: Off-allowlist mirror URL is rejected

- **WHEN** an artifact is recorded with a `mirror_url` whose host is not `github.com` or whose repo differs from the app's configured source
- **THEN** the request is rejected (4xx) and no `mirror_url` is persisted

#### Scenario: Re-upload without a mirror clears the stale URL

- **GIVEN** an artifact with a recorded `mirror_url`
- **WHEN** the same artifact identity is re-uploaded with new bytes and no `mirror_url`
- **THEN** the stored `mirror_url` is cleared (not left pointing at superseded bytes)

### Requirement: Server SHALL register externally-hosted artifacts without an S3 upload

The server SHALL provide a write path that registers an artifact from client-supplied metadata (`platform`, `target?`, `arch?`, `abi?`, `kind`, `filename`, `size_bytes`, `sha256`, optional `signature`) plus a `mirror_url`, WITHOUT a presigned PUT or object-storage round-trip, requiring `artifact:upload`. Such an artifact SHALL have no S3 object (`storage_backend_id` and `object_key` absent) and at least the external `mirror_url` as its delivery location. This path SHALL funnel through the same artifact upsert as the S3 upload path (same identity key and idempotency).

#### Scenario: Register a GitHub-only artifact with no active storage backend

- **GIVEN** no active S3 storage backend
- **WHEN** a principal holding `artifact:upload` registers an artifact with metadata and a valid `mirror_url`
- **THEN** an artifact row is created with `mirror_url` set and no `object_key`/`storage_backend_id`
- **AND** the release can be finalized and served from GitHub

### Requirement: Server SHALL verify a GitHub mirror's liveness and digest before exposing it

Before a GitHub `mirror_url` is exposed to clients (in the catalog, the RN update response, or a `?source=github` redirect), the server SHALL verify that the asset is anonymously reachable AND that its digest matches the artifact's `sha256`. Verification SHALL be cached with a TTL, single-flighted per artifact, and negatively cached, so concurrent requests and draft-window polling do not storm GitHub. An asset that is not yet public (draft window) or whose digest does not match SHALL NOT be exposed as a source; a previously-live asset that later 404s or changes digest SHALL stop being exposed.

#### Scenario: Draft-window asset is not exposed until public

- **GIVEN** a `mirror_url` whose GitHub Release is still a draft (anonymous 404)
- **WHEN** the catalog or update response for that artifact is built
- **THEN** the GitHub source is omitted (users are not redirected into a 404)
- **AND** once the release is promoted to public, the source is exposed

#### Scenario: Digest drift stops exposure

- **GIVEN** a previously-live `mirror_url` whose asset has been replaced with different bytes
- **WHEN** verification re-runs after the cache TTL
- **THEN** the digest mismatch is detected and the GitHub source is no longer exposed

#### Scenario: Verification is single-flighted under load

- **WHEN** many concurrent requests need the same artifact's mirror verified
- **THEN** at most one outbound probe to GitHub is in flight per artifact per TTL window

### Requirement: Download intent SHALL be tagged with its source

Every recorded `download_intent` event SHALL carry a `source` dimension (`oss` or `github`) identifying which delivery location the redirect targeted, persisted (not only logged) so that per-source download volume and fallback health are queryable.

#### Scenario: Source dimension distinguishes OSS from GitHub downloads

- **WHEN** a client downloads via `?source=github` and another via the default OSS path
- **THEN** two `download_intent` events are recorded with `source=github` and `source=oss` respectively

### Requirement: Public catalog and RN update response SHALL carry mirror candidates

The public `DownloadCatalog` SHALL expose, per artifact, the set of available sources as `sources: [{ kind, url }]` (S3 primary plus GitHub mirror when present and verified), and the RN Android update response SHALL carry `mirror_urls: [..]` of verified GitHub candidates. Each candidate URL SHALL route through the `/download/{app}/{version}/{artifact_id}?source=…` indirection (not a raw `github.com` link) so that intent telemetry and liveness gating still apply.

#### Scenario: Catalog renders OSS and GitHub sources

- **GIVEN** a published release whose artifact has a verified GitHub mirror
- **WHEN** the public catalog is fetched
- **THEN** that artifact's `sources` contains both an `oss` and a `github` entry, each pointing at the `?source=` indirection URL

#### Scenario: GitHub-only artifact lists only the GitHub source

- **GIVEN** an artifact with only a `mirror_url` (no S3 object)
- **WHEN** the catalog is fetched
- **THEN** its `sources` contains only the `github` entry
