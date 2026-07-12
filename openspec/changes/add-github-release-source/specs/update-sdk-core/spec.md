## ADDED Requirements

### Requirement: ReleaseInfo SHALL carry mirror candidates and RN download SHALL fail over across sources

The SDK's `ReleaseInfo` SHALL carry an optional ordered list of mirror download URLs, and `normalizeAndroid` SHALL populate it from the RN update response's `mirror_urls`. The reference RN adapter's `download()` SHALL attempt the primary `url` first and, on a download failure, fall through to the mirror candidates in order until one succeeds or all are exhausted. A "download failure" that triggers fall-through SHALL include both a non-APK/error-page response (as already distinguished by the APK download assertion) AND a post-download `sha256` mismatch against the expected value — a same-repo but wrong-bytes asset MUST NOT abort the whole update when a good source remains. When every source fails, the adapter SHALL surface a retryable download error (not a silent success).

#### Scenario: Primary fails, mirror succeeds

- **GIVEN** a `ReleaseInfo` whose primary `url` returns an error page and whose first mirror serves the correct APK
- **WHEN** the RN adapter downloads
- **THEN** it falls through to the mirror and completes the install

#### Scenario: sha256 mismatch falls through instead of aborting

- **GIVEN** a primary source that returns a valid APK whose `sha256` does not match the expected value, and a good mirror
- **WHEN** the RN adapter downloads
- **THEN** the mismatched primary is rejected and the mirror is attempted, rather than aborting the update

#### Scenario: All sources fail surfaces a retryable error

- **GIVEN** a primary and all mirrors failing to deliver matching bytes
- **WHEN** the RN adapter downloads
- **THEN** it surfaces a retryable download error, not a silent success

#### Scenario: No mirrors preserves single-source behavior

- **GIVEN** a `ReleaseInfo` with an empty mirror list
- **WHEN** the RN adapter downloads
- **THEN** it behaves exactly as the pre-existing single-`url` flow
