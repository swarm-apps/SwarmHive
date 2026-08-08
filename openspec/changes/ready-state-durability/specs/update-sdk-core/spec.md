## MODIFIED Requirements

### Requirement: SDK SHALL define the UpdateAdapter ports interface

The package SHALL export an `UpdateAdapter` interface with `check`, `download(onProgress)`,
`install`, `storage: KeyValueStorage`, `compare`, and an **optional** `reconcile` member.
The engine SHALL depend only on this interface, never on any platform API directly. This
interface is the sole contract between the npm package and the platform adapters that live
in the registries.

`reconcile(release: ReleaseInfo | null): Promise<DownloadHandle | null>` SHALL align the
platform's on-disk update artifact with the current candidate release:

- when `release` is non-null and a matching, verified artifact exists locally, it SHALL
  return a `DownloadHandle` usable by `install` without any further download;
- when `release` is non-null but the local artifact does not match it, the implementation
  SHALL discard the stale artifact and return `null`;
- when `release` is `null` (no candidate — already up to date, or the user dismissed it),
  the implementation SHALL discard any local artifact and return `null`.

Adapters that have no recoverable artifact (e.g. Tauri, whose downloaded bundle lives inside
the updater plugin and exposes no path) SHALL omit `reconcile` entirely; the engine SHALL
treat an absent `reconcile` as "never reuse an artifact".

#### Scenario: Engine drives any conforming adapter

- **GIVEN** an in-memory mock adapter implementing `UpdateAdapter`
- **WHEN** the engine runs a full check → download → install cycle
- **THEN** every platform interaction goes through the adapter's methods
- **AND** the engine references no `@tauri-apps`/`expo` symbol

#### Scenario: Adapter without reconcile keeps working

- **GIVEN** an adapter that does not implement `reconcile`
- **WHEN** `engine.check()` finds a newer release
- **THEN** `status` ends at `available` (never `ready`)
- **AND** no error is raised for the missing optional member

### Requirement: createUpdateEngine SHALL implement the 8-state machine

`createUpdateEngine(adapter, opts)` SHALL expose a framework-agnostic store with `status` ∈
{`idle`, `checking`, `up-to-date`, `available`, `force-required`, `downloading`, `ready`,
`error`} and actions `check`, `download`, `install`, `postpone`, `retry`, `acknowledgeError`.
A check that finds a newer version SHALL move to `available` (or `force-required` when the
update is forced); no newer version SHALL move to `up-to-date`; a download error SHALL move
to `error` with a retry path back to `checking`.

`ready` SHALL be a **durable resting state**, not a transient one. It asserts a local fact —
"a verified, installable artifact is on hand" — and SHALL persist until the artifact becomes
unusable. `install()` SHALL NOT leave `ready`, and SHALL NOT be the thing that ends it.

#### Scenario: Check yields available

- **GIVEN** an adapter whose `check` returns a release newer than current and `upgradeType="prompt"`
- **WHEN** `engine.check()` runs
- **THEN** `status` transitions `idle → checking → available`
- **AND** `release` holds the returned `ReleaseInfo`

#### Scenario: Forced update yields force-required

- **GIVEN** an adapter whose `check` returns a release with `upgradeType="force"`
- **WHEN** `engine.check()` runs
- **THEN** `status` ends at `force-required`

#### Scenario: Download error is retryable

- **GIVEN** the engine is `available` and the adapter's `download` rejects
- **WHEN** `engine.download()` runs then `engine.retry()`
- **THEN** `status` goes `downloading → error → checking`

#### Scenario: Check recovers a previously downloaded artifact

- **GIVEN** an adapter whose `reconcile` returns a handle matching the candidate release
- **WHEN** `engine.check()` finds that release
- **THEN** `status` transitions `checking → ready` without ever entering `downloading`
- **AND** `adapter.download` is never called

#### Scenario: Dismissed release is not resurrected, but its artifact is kept

- **GIVEN** a non-forced release the user has postponed within the dismiss TTL
- **AND** an adapter whose `reconcile` would return a matching handle
- **WHEN** `engine.check()` runs
- **THEN** `status` ends at `up-to-date` — the user asked not to be nagged
- **AND** `reconcile` is NOT called, so the downloaded artifact survives the postponement
  and is picked up directly once the TTL expires

#### Scenario: An installed version's artifact is cleaned up

- **GIVEN** an artifact for version N on disk and the app is now running version N
- **WHEN** `engine.check()` finds no newer release
- **THEN** `reconcile(null)` is called so the now-useless artifact is deleted
- **AND** `status` ends at `up-to-date`

## ADDED Requirements

### Requirement: install SHALL be idempotent and SHALL NOT consume the ready state

`install()` SHALL hand the current `DownloadHandle` to `adapter.install` **without clearing
it**, and SHALL leave `status` at `ready` on success. Calling `install()` repeatedly while
`ready` SHALL invoke `adapter.install` each time with the same handle.

This exists because a platform hand-off can fail invisibly: on Android 10+ an
`ACTION_VIEW` install intent dispatched while the app is in the background is silently
dropped by the Background Activity Launch restriction — no exception, no callback, only a
logcat line. An engine that destroys the handle on the first attempt makes that failure
permanent and unrecoverable without re-downloading.

The handle SHALL be discarded only when:

1. `reconcile` reports the artifact is no longer usable (returns `null`), or
2. `retry()` is invoked (the user explicitly asked to start over), or
3. a new `download()` produces a replacement handle.

#### Scenario: Repeated install keeps the handle

- **GIVEN** the engine is `ready` with a handle
- **WHEN** `engine.install()` is called three times
- **THEN** `adapter.install` receives the same handle three times
- **AND** `status` remains `ready` throughout

#### Scenario: Install failure surfaces as error but is retryable

- **GIVEN** the engine is `ready` and `adapter.install` rejects
- **WHEN** `engine.install()` runs
- **THEN** `status` becomes `error` with `phase: "install"`
- **AND** the handle is retained

### Requirement: acknowledgeError SHALL restore the state the artifact justifies

`acknowledgeError()` SHALL resolve its target status from whether an installable artifact is
still on hand, not from `release` alone:

| condition | restored status |
|---|---|
| a handle is held | `ready` |
| no handle, no release | `idle` |
| no handle, forced release | `force-required` |
| no handle, normal release | `available` |

Without this, dismissing an install error would drop the engine to `available` while the
artifact is still on disk — and `install()` (which requires `ready`) could never be reached
again, forcing a pointless re-download of bytes the device already has.

#### Scenario: Dismissing an install error returns to ready

- **GIVEN** the engine is `error` with `phase: "install"` and still holds a handle
- **WHEN** `engine.acknowledgeError()` runs
- **THEN** `status` becomes `ready`
- **AND** `engine.install()` works again without any download

#### Scenario: Dismissing a check error returns to available

- **GIVEN** the engine is `error` with `phase: "check"`, a normal release, and no handle
- **WHEN** `engine.acknowledgeError()` runs
- **THEN** `status` becomes `available`

#### Scenario: retry discards the artifact

- **GIVEN** the engine is `ready` with a handle
- **WHEN** `engine.retry()` runs
- **THEN** the handle is discarded and `status` goes `idle → checking`
