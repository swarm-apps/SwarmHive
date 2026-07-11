## ADDED Requirements

### Requirement: Artifact SHALL support one or more delivery locations

An artifact SHALL model its bytes as one or more delivery locations rather than a single mandatory S3 object. `storage_backend_id` and `object_key` SHALL be nullable (present together for an S3-backed object, absent otherwise), and an optional `mirror_url` SHALL hold an external delivery location (currently the GitHub Release asset URL). Every artifact MUST have at least one delivery location — an S3 object, a `mirror_url`, or both. The identity/uniqueness key of an artifact `(release_id, platform, target, arch, abi, kind)` SHALL be unchanged; delivery-location columns are descriptive and MUST NOT be part of that key. `api::Artifact` SHALL reflect the nullable S3 fields and the optional `mirror_url`.

#### Scenario: S3-backed and GitHub-only artifacts coexist under one release

- **WHEN** one artifact is uploaded to S3 and another is registered with only a `mirror_url`
- **THEN** both persist under the same release, each with at least one delivery location
- **AND** neither violates the `(release_id, platform, target, arch, abi, kind)` uniqueness key

#### Scenario: An artifact with no delivery location is invalid

- **WHEN** an attempt is made to persist an artifact with neither an S3 object nor a `mirror_url`
- **THEN** it is rejected (no delivery location)

## MODIFIED Requirements

### Requirement: Server SHALL expose read-only Artifact and current-release queries

The server SHALL expose `GET /api/v1/apps/:slug/releases/:version/artifacts` (requires `artifact:read`) returning the artifacts of a release, and `GET /api/v1/apps/:slug/channels/:name/release` (requires `release:read`) returning the release a channel currently serves (or empty when the channel has never been promoted). The server SHALL NOT expose artifact create/delete endpoints in this capability (artifact creation is the upload `complete` callback's job or the external-registration path, deferred to storage). Artifact listings SHALL include each artifact's available delivery locations (S3 presence and any `mirror_url`).

#### Scenario: Listing artifacts of a release

- **GIVEN** a release with artifacts present
- **WHEN** a principal holding `artifact:read` GETs `.../releases/:version/artifacts`
- **THEN** the response lists the artifacts with platform / target / arch / abi / filename / size_bytes / sha256
- **AND** each artifact indicates its delivery locations (S3 and/or `mirror_url`)

#### Scenario: Querying a never-promoted channel's current release

- **GIVEN** a channel with no `channel_release` row
- **WHEN** a principal GETs `/api/v1/apps/:slug/channels/:name/release`
- **THEN** the response indicates no current release (empty / 204-style payload), not a 404 on the channel
