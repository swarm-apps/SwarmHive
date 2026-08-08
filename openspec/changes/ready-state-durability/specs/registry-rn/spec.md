## ADDED Requirements

### Requirement: The downloader SHALL NOT claim resumability it cannot deliver

`createExpoApkDownloader` SHALL download in full and SHALL NOT persist a resume record.

Expo's `DownloadResumable` looks resumable but is not, for our failure mode: `resumeData` is
assigned **only** inside `pauseAsync()` (`this.resumeData = pauseResult.resumeData`), and
`savable()` merely reads the current fields back out. A process that is killed gets no chance
to call `pauseAsync`, so a record saved before the download starts always carries
`resumeData: undefined`. Handing that back to `resumeAsync()` sends no `Range` header and the
native layer truncates the target file — a full re-download wearing a resumed download's
clothes, plus a stale partial file left on disk because the "we have a breakpoint" branch
skipped the cleanup.

Genuine resumability would require pausing on every foreground exit and immediately resuming,
which defeats the point of downloading while the screen is off. **Reuse of a
already-completed artifact across processes is covered by `reconcile`, and that path is real.**

#### Scenario: A leftover partial is discarded, not resumed

- **GIVEN** a partial file from a previous killed download
- **WHEN** `download()` runs for the same release
- **THEN** the leftover is deleted and the APK is fetched in full
- **AND** no resume record is read or written

### Requirement: The RN adapter SHALL implement reconcile

`createRnAdapter` SHALL implement the SDK's optional `reconcile(release)` port so a verified
APK left on disk by a previous process becomes directly installable.

`reconcile` SHALL return a handle only when **all** of the following hold: a persisted
artifact record exists, its recorded version matches `release`, the file exists, and it
passes the same verification the downloader applies at completion (byte size against
`release.sizeBytes` when available, plus ZIP magic). Otherwise it SHALL delete the artifact
and its record, and return `null`.

When called with `release === null` it SHALL delete any artifact and record, and return
`null` — this is the path that removes an APK the user has already installed.

#### Scenario: A completed download survives an app restart

- **GIVEN** an APK for version N was downloaded and verified, then the app was killed
- **WHEN** the app restarts and `check()` returns version N
- **THEN** `reconcile` returns a handle and the engine enters `ready` without downloading

#### Scenario: The artifact is removed once its version is installed

- **GIVEN** an APK for version N is on disk and the user has installed it
- **WHEN** `check()` reports no update available and the engine calls `reconcile(null)`
- **THEN** the APK file and its record are deleted

#### Scenario: A corrupted artifact is not offered

- **GIVEN** a persisted record for version N whose file was truncated by external means
- **WHEN** `reconcile` is called for version N
- **THEN** it deletes the file and returns `null`, and the engine falls back to `available`

### Requirement: APK install SHALL be gated before dispatch

`createExpoApkInstaller` SHALL refuse to dispatch the install intent when a gate fails, and
SHALL reject with a distinguishable `ApkInstallBlockedError` carrying a `reason`, rather than
dispatching an intent that will silently vanish.

**Foreground gate** — when `AppState.currentState !== "active"`, it SHALL NOT call
`startActivityAsync` and SHALL reject with reason `"background"`. Android 10+ silently drops
activity launches from the background; dispatching anyway produces a *resolved* promise and a
UI that claims a system dialog is open when none is.

There SHALL be **no permission gate**. `expo-intent-launcher` exposes no
`canRequestPackageInstalls` query, so any built-in check would be a guess, and a wrong
"denied" verdict blocks an install that would have worked. When the user has not allowed
installs from this source, Android's own consent screen takes over — they grant it, come back,
and `ready` is still there. `ApkInstallBlockReason` therefore has exactly one member; adding a
value the code cannot produce would grow a UI branch that is never reached and imply guidance
that does not exist.

`ApkInstallBlockedError` SHALL be defined in `ports.ts`, not in the expo-specific module, so
UI components can discriminate on it without importing any `expo-*` symbol.

A blocked install is **not** a failed install: the artifact is untouched and the engine stays
`ready`, so retrying once the gate opens costs nothing.

#### Scenario: Install attempted while the app is backgrounded

- **GIVEN** the app is not in the foreground (e.g. the screen is off)
- **WHEN** `install()` is invoked
- **THEN** no intent is dispatched
- **AND** the caller receives `ApkInstallBlockedError` with reason `"background"`
- **AND** the artifact is retained so the attempt can be repeated later

#### Scenario: Unknown-sources permission not granted

- **GIVEN** the user has not allowed installing apps from this source
- **WHEN** `install()` is invoked while in the foreground
- **THEN** the intent is dispatched anyway and Android shows its own consent screen
- **AND** after granting it the user returns to an app still in `ready`, one tap from installing

### Requirement: The ready state SHALL auto-attempt install once per release on foreground entry

registry-rn SHALL provide a hook (`useAutoInstall`) that triggers `install()` when the app
transitions into the foreground while `status === "ready"`, **at most once per release
version per process**.

The once-per-release bound is required because dispatching the install intent moves the app
out of the foreground; a user who cancels the system dialog returns to `active` and would
otherwise retrigger the dialog indefinitely. The bound SHALL be process-local (in-memory) —
one automatic attempt per app launch is desirable, so it deliberately does not persist.

After the automatic attempt is spent, the ready-state primary button is the user's manual
path, and it SHALL be enabled (see the UI contract requirement).

#### Scenario: Download finishes while the screen is off

- **GIVEN** the download completes while the app is backgrounded
- **WHEN** the user later brings the app to the foreground
- **THEN** `install()` is invoked once and the system install dialog appears

#### Scenario: User cancels the system dialog

- **GIVEN** the automatic attempt has been made and the user cancelled the system dialog
- **WHEN** the app returns to the foreground
- **THEN** `install()` is NOT invoked automatically again
- **AND** the UI shows the "cancelled — you can try again" hint with an enabled install button

### Requirement: Update UI SHALL have no dead ends

Every update UI state in registry-rn SHALL offer at least one user-actionable exit:

1. In `ready`, the primary button SHALL be **enabled** and SHALL invoke `install()`. It SHALL
   NOT be disabled by a `busy` predicate that lumps `ready` together with `downloading`.
2. `update-progress-dialog` SHALL be dismissable. Dismissing it SHALL only hide the UI — it
   SHALL NOT cancel the download nor change `status`. A controlled `open` prop SHALL always be
   paired with an `onOpenChange` handler.
3. `update-settings-section` SHALL render a distinct affordance for `ready` (install entry),
   and SHALL NOT let `ready` fall through to an "up to date" branch.

The force-update dialog remains the single intentional exception to dismissability, but is
NOT exempt from rule 1 — a forced user with a disabled button has no exit at all.

#### Scenario: Ready-state button is actionable

- **WHEN** `status === "ready"` in the prompt dialog, the force dialog, or the settings section
- **THEN** the primary button is enabled and pressing it invokes `install()`

#### Scenario: Progress dialog can be dismissed without cancelling

- **GIVEN** the progress dialog is showing during `downloading`
- **WHEN** the user dismisses it (back gesture or explicit control)
- **THEN** the dialog closes, the download continues, and `status` is unchanged

### Requirement: Ready-state copy SHALL describe the local fact, not an unobservable system state

`update-texts.ts` SHALL replace the ready-state hint with one that states what is true locally
and what the user can do, since the app cannot observe whether the system dialog actually
appeared — and in the background-blocked case it demonstrably did not.

`ready` can be occupied for more than one reason, and they mean different next steps to the
user. The hint SHALL be selected by a pure, testable predicate (`readyHintKind`) with the copy
mapping kept separate (`readyHintText`), over exactly three cases:

| case | when | copy |
|---|---|---|
| `background` | the install gate rejected — the intent was never dispatched | "回到应用即可继续安装。" |
| `canceled` | the automatic attempt is spent and we are still `ready` | "已取消安装，可以再试一次" |
| `ready` | nothing has been tried yet | "更新已就绪，点击安装" |

Copy keys that no longer have a producer SHALL be removed rather than left in the interface:
`systemConfirmHint` (replaced by `readyHint`), `unknownSourceHint` (no permission gate exists
to raise it) and `restartingButton` (both call sites moved to `installButton`). A required
member of the exported `UpdateTexts` that nothing ever renders forces every downstream
override to supply a dead string.

#### Scenario: Ready hint no longer claims a system dialog is open

- **WHEN** `status === "ready"` and nothing has been attempted
- **THEN** the hint reads "更新已就绪，点击安装" / "Update ready — tap to install"

#### Scenario: A spent auto-attempt reads as a cancellation

- **GIVEN** the automatic attempt was made, no gate rejected it, and `status` is still `ready`
- **THEN** the hint reads "已取消安装，可以再试一次" and the install button stays enabled
