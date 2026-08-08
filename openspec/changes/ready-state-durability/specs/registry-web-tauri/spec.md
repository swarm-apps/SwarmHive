## ADDED Requirements

### Requirement: Update UI SHALL have no dead ends

Every update UI state in registry-web-tauri SHALL offer at least one user-actionable exit.
The Tauri install path is synchronous and usually leaves `ready` within a frame, but it is not
guaranteed to: on Windows the passive installer can be dismissed at the UAC prompt, and any
`install()` rejection parks the flow. The same three rules therefore apply here as in
registry-rn — one contract, two platforms.

1. In `ready`, the primary button SHALL be **enabled** and SHALL invoke `install()`. It SHALL
   NOT be disabled by a `busy` predicate that lumps `ready` together with `downloading`.
2. `update-progress-dialog` SHALL be dismissable. It SHALL NOT suppress both
   `onPointerDownOutside` and `onEscapeKeyDown` while offering no footer action — that
   combination is a modal with no exit. Dismissing SHALL only hide the UI: the download
   continues and `status` is unchanged.
3. `update-settings-section` SHALL render a distinct affordance for `ready`, and SHALL NOT let
   `ready` fall through to an "up to date" branch.

The force-update dialog remains the single intentional exception to dismissability, but is
NOT exempt from rule 1.

#### Scenario: Ready-state button is actionable

- **WHEN** `status === "ready"` in the prompt dialog, the force dialog, or the settings section
- **THEN** the primary button is enabled and clicking it invokes `install()`

#### Scenario: Progress dialog can be dismissed without cancelling

- **GIVEN** the progress dialog is showing during `downloading`
- **WHEN** the user presses Escape or clicks outside
- **THEN** the dialog closes, the download continues, and `status` is unchanged

#### Scenario: Install rejection leaves a usable path

- **GIVEN** `install()` rejected (e.g. the user cancelled the Windows UAC prompt)
- **WHEN** the user acknowledges the error
- **THEN** the engine returns to `ready` and the enabled primary button retries the install
  without re-downloading

### Requirement: Progress presentation SHALL distinguish downloading from ready

The progress dialog SHALL NOT present `ready` as if it were still transferring. When
`status === "ready"` it SHALL stop the spinner, drop the transfer-rate readout, and title
itself by the ready-state copy rather than the download title.

A dialog showing "Downloading update · 100% · 1.0 MB/s" after the download finished is
reporting a stale last frame, not a live state.

#### Scenario: Ready state stops presenting transfer telemetry

- **WHEN** `status` moves from `downloading` to `ready`
- **THEN** the spinner stops and the speed readout is removed
- **AND** the title reflects readiness, not download progress
