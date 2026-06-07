## MODIFIED Requirements

### Requirement: CLI SHALL verify and publish artifacts via presign + complete

The `swarmhive` CLI SHALL provide `verify tauri|android` and `publish tauri|android`. `verify` SHALL check artifact existence, parse `latest.json` (Tauri), compute sha256, and query the server for a duplicate version; it SHALL trust `--version`/`--version-code` flags rather than parsing APK binaries or `build.gradle`. `publish` SHALL read `swarmhive.toml` (single app, `--app` override; Tauri version auto-read from `tauri.conf.json`, Android version via explicit flags), ensure a draft release exists, presign, stream-upload each file with a progress bar sending `x-amz-checksum-sha256`, retry transient failures per file, and call complete (default `publish=true`). When given `--notes-file <path>` (or `--notes <text>`, with file taking precedence), `publish` SHALL inject the release notes into the release — written through `CreateReleaseRequest` on a freshly created draft, and through the release update (PATCH) endpoint when the draft already exists — reusing existing server endpoints with no server changes. When given `--dry-run`, `publish` SHALL perform only the local plan (locate artifacts, compute sha256, detect the sibling `.sig`, print the plan) and SHALL NOT make any network call or require credentials. Both `verify` and `publish` SHALL honor the global `--output {table|json}`: with `--output json` a successful result SHALL print as a single JSON object to stdout and the upload progress bar SHALL be suppressed; failures continue to emit RFC 9457 problem+json to stderr with a non-zero exit. The progress bar SHALL also be suppressed when stderr is not a TTY. The HTTP client SHALL use rustls with system root certs and honor `--ca-cert`/`SWARMHIVE_CA_CERT`.

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

#### Scenario: Publish injects release notes from a changelog file

- **GIVEN** a logged-in CLI and a `CHANGELOG.md`
- **WHEN** the user runs `swarmhive publish tauri --app swarmdrop --notes-file CHANGELOG.md`
- **THEN** a freshly created draft carries the notes via the create request, while an already-existing draft is updated via the release PATCH endpoint
- **AND** the resulting release's `release_notes` is non-empty

#### Scenario: Publish dry-run plans locally without uploading

- **WHEN** the user runs `swarmhive publish android --app swarmnote-rn --version 0.2.1 --version-code 21 --apk <path> --dry-run`
- **THEN** the CLI prints the planned release, artifacts, sha256, and target channel without contacting the server
- **AND** no presign/upload/complete request is made and no credentials are required

#### Scenario: JSON output emits a single object with no progress noise

- **WHEN** the user runs `swarmhive publish tauri --app swarmdrop --output json` (or `verify ... --output json`)
- **THEN** stdout carries exactly one JSON object describing the result and the upload progress bar is suppressed
- **AND** an API error instead writes RFC 9457 problem+json to stderr with a non-zero exit
