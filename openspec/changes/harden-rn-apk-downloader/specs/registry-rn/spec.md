## ADDED Requirements

### Requirement: The APK downloader SHALL validate the delivery before resolving

`createExpoApkDownloader` SHALL NOT resolve a local path until it has established that the downloaded file is actually an APK. `createDownloadResumable` does NOT reject on a non-2xx response — it writes the error body to the target file and resolves normally — so a downloader that only checks for a resolved URI will hand an error page to the system installer. Before resolving, the downloader SHALL reject when: the HTTP status is outside 2xx; the file is missing or shorter than 4 bytes; an expected `sizeBytes` was supplied and the file's actual size differs from it; or the file's leading bytes are not the ZIP magic that every APK begins with. On any rejection the downloader SHALL delete the partial file before throwing, so a poisoned file cannot be resumed or installed later. The rejection message SHALL carry the HTTP status, the response content-type, and a short body preview when available, because these failures are otherwise indistinguishable at the call site.

These rejections are what triggers the adapter's mirror fall-through — validation and failover are one mechanism, and a downloader that skips validation silently disables failover for the exact failure it exists to survive.

#### Scenario: A 200 error page is rejected rather than installed

- **GIVEN** an object store that returns HTTP 200 with an XML error body instead of APK bytes (an anonymous-APK-download restriction)
- **WHEN** the downloader fetches that URL
- **THEN** it rejects because the leading bytes are not ZIP magic, and does not resolve a path to the XML file

#### Scenario: Rejection triggers the adapter's fall-through to a mirror

- **GIVEN** a primary URL serving a 200 XML error page and a mirror serving the real APK
- **WHEN** the RN adapter downloads
- **THEN** the downloader's rejection makes the adapter fall through, and the update completes from the mirror

#### Scenario: A truncated delivery is rejected on size

- **GIVEN** a download that resolves with valid ZIP magic but fewer bytes than the expected `sizeBytes`
- **WHEN** the downloader validates it
- **THEN** it rejects, because leading bytes alone cannot reveal truncation

#### Scenario: A poisoned file is deleted, not left for resume

- **GIVEN** any validation failure
- **WHEN** the downloader throws
- **THEN** the partial file has been deleted first

#### Scenario: Omitting the expectation keeps the other checks

- **GIVEN** a caller that supplies no expected `sizeBytes`
- **WHEN** the downloader validates a delivery
- **THEN** the size check is skipped and the status, non-empty, and ZIP-magic checks still apply

#### Scenario: A valid APK resolves unchanged

- **GIVEN** a 2xx response whose body is a real APK matching the expected size
- **WHEN** the downloader validates it
- **THEN** it resolves the local path exactly as before this requirement existed

### Requirement: registry-rn SHALL be the upstream source of truth for the components it ships

The components under `registry/rn/` SHALL be authoritative, and consuming apps (SwarmDrop-RN, SwarmNote-RN, and any new app) SHALL obtain them by pulling from this registry rather than maintaining independently-evolving copies. Registry component headers SHALL NOT describe themselves as mirroring a downstream app — that inversion removes any obligation for downstream hardening to flow back upstream, and is what allowed the APK download validation to exist only in a consumer while the registry shipped an unprotected downloader to every new app.

Every component under `registry/rn/lib/` that carries correctness logic SHALL be covered by this package's own vitest suite, including implementations that depend on `expo-*` (via module mocks). A component that is untested here has no defense against re-drifting.

#### Scenario: Registry headers point downstream, not upstream

- **GIVEN** a component shipped by registry-rn
- **WHEN** its header documents its provenance
- **THEN** it identifies itself as the upstream source that consumers pull from, not as a mirror of a consuming app

#### Scenario: An expo-dependent component is still covered here

- **GIVEN** `createExpoApkDownloader`, which depends on `expo-file-system` and `react-native`
- **WHEN** the registry-rn test suite runs
- **THEN** it exercises the downloader's validation with those modules mocked, rather than deferring all coverage to consuming apps

#### Scenario: Removing a protection turns the suite red

- **GIVEN** the downloader's validation step
- **WHEN** it is removed
- **THEN** the registry-rn suite fails — the coverage tracks the protection itself, not merely the surrounding branch logic
