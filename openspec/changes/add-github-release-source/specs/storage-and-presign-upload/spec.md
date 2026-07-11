## MODIFIED Requirements

### Requirement: Server SHALL serve downloads via redirect and record intent

The server SHALL expose `GET /download/:app/:version/:artifact_id` (public or read-only app key). It SHALL record a `download_intent` (tagged with the resolved `source`), then return `302` to the selected delivery location without proxying bytes. Source selection SHALL honor an optional `?source=oss|github` query: `oss` (or absent) SHALL resolve the artifact's S3 object via the active backend's `url_mode` (`public` or `signed`); `github` — or `oss` when the artifact has no S3 object — SHALL resolve the artifact's verified `mirror_url` (an unconditionally public URL; `url_mode`/TTL do not apply). The endpoint SHALL NOT require an active storage backend when a usable `mirror_url` exists; it SHALL return `409 type=storage_not_configured` ONLY when the artifact has no usable delivery location at all. Artifacts of a `yanked` release SHALL return `404`.

#### Scenario: Download redirects to a storage URL

- **GIVEN** a published release with an artifact backed by S3
- **WHEN** a client GETs `/download/swarmdrop/0.4.5/<artifact_id>` (no `source`)
- **THEN** a `download_intent` is recorded with `source=oss`
- **AND** the response is `302` to a public or signed object URL (no byte proxying)

#### Scenario: Download redirects to a GitHub mirror when requested

- **GIVEN** a published release whose artifact has a verified `mirror_url`
- **WHEN** a client GETs `/download/swarmdrop/0.4.5/<artifact_id>?source=github`
- **THEN** a `download_intent` is recorded with `source=github`
- **AND** the response is `302` to the GitHub asset URL

#### Scenario: GitHub-only artifact serves without an active backend

- **GIVEN** an artifact with only a `mirror_url` and no active S3 backend
- **WHEN** a client GETs `/download/...` for it
- **THEN** the response is `302` to the GitHub asset URL (not `409`)

#### Scenario: Download with no usable source is not-configured

- **GIVEN** an artifact with no S3 object and no usable `mirror_url`
- **WHEN** a client GETs `/download/...` for it
- **THEN** the response is `409 type=storage_not_configured`

#### Scenario: Download of a yanked release is gone

- **GIVEN** a release that has been yanked
- **WHEN** a client GETs `/download/...` for one of its artifacts
- **THEN** the response is `404`
