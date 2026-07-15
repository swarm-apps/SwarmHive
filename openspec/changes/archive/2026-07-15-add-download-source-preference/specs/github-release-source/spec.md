## MODIFIED Requirements

### Requirement: Server SHALL manage a per-app GitHub Release source

The server SHALL expose CRUD for a single GitHub Release download source per app, requiring `app:update`. The config SHALL hold `owner`, `repo`, `tag_template` (default `v{version}`), `enabled`, an optional access token, and `prefer_for_platforms` — the set of artifact platforms for which this GitHub source SHALL be preferred over OSS when no explicit source is requested. `prefer_for_platforms` SHALL default to empty (every platform prefers OSS — the pre-existing behavior) and SHALL be validated at store time against the known platform enum, rejecting unknown values rather than persisting a silently-ineffective config. The access token SHALL be encrypted at rest via the server secret key and MUST NEVER be returned by any API; the view SHALL expose only `token_set: bool`. At most one GitHub source SHALL exist per app, enforced by a full unique constraint on `app_id` (NOT a partial index). The token SHALL be used ONLY for server-side liveness/digest probing and rate-limit relief — it SHALL NOT be used to deliver bytes to clients.

#### Scenario: Create and read a GitHub source hides the token

- **GIVEN** an owner creating a GitHub source with an access token
- **WHEN** the source is read back
- **THEN** the view exposes `token_set: true` and never the token itself

#### Scenario: Second GitHub source per app is rejected

- **GIVEN** an app that already has a GitHub source
- **WHEN** a second source is created for the same app
- **THEN** the request is rejected by the unique constraint on `app_id`

#### Scenario: Blank token on update keeps the stored token

- **GIVEN** a source with a stored access token
- **WHEN** it is updated with a blank token field
- **THEN** the previously stored token is retained

#### Scenario: Platform preference is stored and defaults to empty

- **GIVEN** a GitHub source created without `prefer_for_platforms`
- **WHEN** it is read back
- **THEN** `prefer_for_platforms` is empty, meaning every platform prefers OSS

#### Scenario: Unknown platform in the preference is rejected

- **GIVEN** an update carrying a `prefer_for_platforms` entry that is not a known platform
- **WHEN** the request is submitted
- **THEN** it is rejected with a validation error, rather than persisting a config that could never take effect

### Requirement: Public catalog and RN update response SHALL carry mirror candidates

The public `DownloadCatalog` SHALL expose, per artifact, the set of available sources as `sources: [{ kind, url }]` (S3 primary plus GitHub mirror when present and verified), and the RN Android update response SHALL carry `mirror_urls: [..]`. Both SHALL be ordered by the app's resolved preference for that artifact's platform, so that the recommended source appears first. `mirror_urls` SHALL contain the available sources OTHER THAN the one the primary `download_url` resolves to, in fallback order — it SHALL NOT list the preferred source itself, because a client would otherwise attempt the same delivery location twice. Each candidate URL SHALL route through the `/download/{app}/{version}/{artifact_id}?source=…` indirection (not a raw `github.com` link) so that intent telemetry and liveness gating still apply.

#### Scenario: Catalog renders sources in preference order

- **GIVEN** an app preferring GitHub for `react-native-android` with both an S3 object and a verified mirror
- **WHEN** the public catalog is fetched
- **THEN** the APK artifact lists the GitHub source first and the OSS source second

#### Scenario: GitHub-only artifact lists only the GitHub source

- **GIVEN** an artifact registered with a mirror and no S3 object
- **WHEN** the public catalog is fetched
- **THEN** only the GitHub source is listed

#### Scenario: Preferring GitHub makes OSS the fallback candidate

- **GIVEN** an app preferring GitHub for `react-native-android`, with a verified mirror and an S3 object under an active backend
- **WHEN** the RN update response is built
- **THEN** `mirror_urls` contains the `?source=oss` candidate and NOT the GitHub one, because `download_url` already resolves to GitHub

#### Scenario: Unconfigured app keeps the pre-existing response byte-for-byte

- **GIVEN** an app with no `prefer_for_platforms` configured
- **WHEN** the RN update response is built
- **THEN** `download_url` resolves to OSS and `mirror_urls` carries the verified `?source=github` candidate, exactly as before this change

## ADDED Requirements

### Requirement: Download source resolution SHALL follow explicit request, then configured preference, then OSS-first

When resolving which delivery location a `/download` request redirects to, the server SHALL order its candidates by: an explicit `?source=` parameter first (highest precedence — an explicit request SHALL NOT be overridden by configuration), otherwise the app's `prefer_for_platforms` for that artifact's platform, otherwise OSS-first. The server SHALL then redirect to the first candidate that is actually usable, preserving the pre-existing fallback semantics unchanged: an OSS candidate requires an object key plus an active backend, and a GitHub candidate requires a mirror URL that passes the liveness/digest gate. A configured preference SHALL NOT be able to produce a dead link — when the preferred source is unusable (mirror not yet public, digest drift, or the source disabled), resolution SHALL fall through to the remaining candidate rather than failing.

#### Scenario: Configured preference redirects to GitHub without any client change

- **GIVEN** an app preferring GitHub for `react-native-android`, with a verified mirror
- **WHEN** a client requests the bare `/download/{app}/{version}/{artifact_id}` with no `?source`
- **THEN** it is redirected to the GitHub asset and the `download_intent` event records `source = github`

#### Scenario: Preference is scoped to the platform it names

- **GIVEN** an app preferring GitHub only for `react-native-android`, holding both an APK and a desktop artifact
- **WHEN** the desktop artifact is requested with no `?source`
- **THEN** it is redirected to OSS, because the preference does not name its platform

#### Scenario: Explicit source outranks the configured preference

- **GIVEN** an app preferring GitHub for `react-native-android`
- **WHEN** a client requests `?source=oss` for an APK
- **THEN** it is redirected to OSS, because an explicit request outranks configuration

#### Scenario: Preferred-but-unusable source falls through instead of failing

- **GIVEN** an app preferring GitHub for `react-native-android` whose mirror fails the liveness gate, and a usable S3 object
- **WHEN** the bare download entry is requested
- **THEN** it is redirected to OSS rather than returning a conflict

#### Scenario: Disabled source cannot be preferred into a dead link

- **GIVEN** an app preferring GitHub for `react-native-android` whose GitHub source is `enabled: false`
- **WHEN** the bare download entry is requested
- **THEN** it is redirected to OSS, because a disabled source is never a usable candidate
