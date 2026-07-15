## MODIFIED Requirements

### Requirement: ReleaseInfo SHALL carry mirror candidates and RN download SHALL fail over across sources

The SDK's `ReleaseInfo` SHALL carry an optional ordered list of mirror download URLs, and `normalizeAndroid` SHALL populate it from the RN update response's `mirror_urls`. `ReleaseInfo` SHALL also carry an optional `sizeBytes`, populated by `normalizeAndroid` from the RN update response's `size_bytes`, so that the downloader can reject a truncated delivery. The reference RN adapter's `download()` SHALL attempt the primary `url` first and, on a download failure, fall through to the mirror candidates in order until one succeeds or all are exhausted. When every source fails, the adapter SHALL surface a retryable download error (not a silent success).

A "download failure" that triggers fall-through SHALL be whatever the injected downloader rejects with. The downloader — NOT the adapter — SHALL own delivery validation, so that the adapter remains pure, injectable logic with no dependency on `expo-*` (see the registry-rn downloader-validation requirement for what it MUST reject). The adapter SHALL pass the expected `sizeBytes` through to the downloader.

Client-side `sha256` verification SHALL NOT be required, superseding the earlier requirement that a post-download `sha256` mismatch trigger fall-through. That requirement was never implementable at acceptable cost and was redundant with an existing server-side gate:

- **No cheap path exists.** `expo-file-system` exposes only md5 (native, streaming); `expo-crypto`'s `digestStringAsync` is one-shot and requires the entire APK as a JS string (a 50MB APK ≈ 67MB base64 in the JS heap); a native streaming-hash dependency would violate registry-rn's established no-native-code constraint.
- **The server already does it, better.** The `github-release-source` capability requires the server to verify a mirror's digest against `artifact.sha256` BEFORE exposing it as a candidate — once, cached, single-flighted. The "same-repo but wrong-bytes asset" scenario the old requirement targeted is that gate's exact purpose; such an asset never becomes a candidate.
- **Residual risk is covered.** Transport corruption is caught by the size and ZIP-magic checks; APK authenticity is enforced by Android's PackageInstaller signature verification, which is the real integrity gate.

#### Scenario: Primary fails, mirror succeeds

- **GIVEN** a `ReleaseInfo` whose primary `url` returns an error page and whose first mirror serves the correct APK
- **WHEN** the RN adapter downloads
- **THEN** it falls through to the mirror and completes the install

#### Scenario: All sources fail surfaces a retryable error

- **GIVEN** a primary and all mirrors failing to deliver a valid APK
- **WHEN** the RN adapter downloads
- **THEN** it surfaces a retryable download error, not a silent success

#### Scenario: No mirrors preserves single-source behavior

- **GIVEN** a `ReleaseInfo` with an empty mirror list
- **WHEN** the RN adapter downloads
- **THEN** it behaves exactly as the pre-existing single-`url` flow

#### Scenario: Expected size reaches the downloader

- **GIVEN** a `ReleaseInfo` normalized from a response carrying `size_bytes`
- **WHEN** the RN adapter downloads
- **THEN** it passes the expected `sizeBytes` to the injected downloader, which owns the validation

#### Scenario: Adapter stays free of expo dependencies

- **GIVEN** the adapter's download path
- **WHEN** its unit tests run
- **THEN** they exercise it with a fake downloader and no `expo-*` module is required, because validation lives in the downloader
