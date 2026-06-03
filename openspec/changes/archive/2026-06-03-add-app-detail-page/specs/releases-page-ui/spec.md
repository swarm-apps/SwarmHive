## REMOVED Requirements

### Requirement: Admin SHALL select an app before listing releases

**Reason**: app 不再由 `/releases` 页的 `?app=<slug>` 全局下拉选择，而是由 App 详情路由 `/apps/:slug` 的 path param 承载（见 `app-detail-navigation`）。`/releases` 顶层页、顶层「版本」菜单项、`?app=` 选择器一并移除，从根本上解决「滚动后丢失 app 上下文」。
**Migration**: 通过 `/apps/:slug/releases` 访问某 app 的版本；无 app 时先在 `/apps` 创建（apps 列表的空态已指向 `/apps`）。

## MODIFIED Requirements

### Requirement: Admin SHALL list releases with lifecycle state

For the app in scope (from the `/apps/:slug` detail route's path param, not a `?app=` selector) the 版本 tab SHALL render a table from `GET /api/v1/apps/:slug/releases` showing version, Android version code (when present), status (draft / published / yanked), published-at, and created-at.

#### Scenario: Releases render with status

- **GIVEN** the app in scope has a draft and a published release
- **WHEN** the 版本 tab table renders
- **THEN** each row shows its version and a status indicator matching draft / published

### Requirement: Admin SHALL create a draft release

The 版本 tab SHALL provide a create form (version, optional Android version code, optional release notes) that POSTs `/api/v1/apps/:slug/releases`, gated on `release:create`. A duplicate version (`409` conflict) SHALL surface a "version already exists" message. On success the list SHALL refresh and the new release SHALL appear as draft.

#### Scenario: Creating a release adds a draft row

- **GIVEN** an authenticated user holding `release:create`
- **WHEN** the user submits the create form with version `1.2.0`
- **THEN** the list refreshes and shows `1.2.0` with status draft

#### Scenario: Duplicate version is reported

- **GIVEN** version `1.2.0` already exists for the app
- **WHEN** the user submits the create form with version `1.2.0`
- **THEN** a "version already exists" message is shown and no row is added

### Requirement: Admin SHALL publish and yank releases with state-aware affordances

The 版本 tab SHALL offer a publish action only on draft rows (gated on `release:publish`) calling `POST .../publish`, and a yank action only on published rows (gated on `release:yank`) calling `POST .../yank`. Affordances SHALL be hidden when the row's state or the user's permissions disallow the action; the server `409` for an illegal transition SHALL be surfaced as a message if it still occurs.

#### Scenario: Publishing a draft moves it to published

- **GIVEN** a draft release and a user holding `release:publish`
- **WHEN** the user confirms publish
- **THEN** the release becomes published and shows a published-at time

#### Scenario: Yank is not offered on a draft

- **GIVEN** a draft release
- **WHEN** the row's actions render
- **THEN** no yank action is shown for that row

#### Scenario: User without publish permission sees no publish action

- **GIVEN** a draft release and a user lacking `release:publish`
- **WHEN** the row's actions render
- **THEN** no publish action is shown

### Requirement: Admin SHALL view a release's artifacts read-only

From a row action the 版本 tab SHALL open a view backed by `GET /api/v1/apps/:slug/releases/:version/artifacts` listing each artifact's platform, target/arch/abi (as present), filename, size, and sha256, **grouped by platform** so a version's multi-platform artifacts are legible at a glance. Browser direct-upload affordances (multi-file drag, automatic platform/target/abi classification, `.sig` pairing) live in this view for users holding `artifact:upload`.

#### Scenario: Artifacts view lists binaries grouped by platform

- **GIVEN** a release with a `tauri-desktop` and a `react-native-android` artifact
- **WHEN** the user opens its artifacts view
- **THEN** artifacts are shown grouped under their platform, each with filename, size, and sha256

### Requirement: Admin SHALL manage channel release pointers (promote / rollback)

For the app in scope the 渠道 tab SHALL show, per channel, the release the channel currently points at (`GET .../channels/:name/release`, which may be empty). It SHALL allow promoting a published release to a channel (`POST .../channels/:name/promote`, gated on `release:promote`; candidate versions limited to published releases) and rolling back (`POST .../channels/:name/rollback`, gated on `release:rollback`). A rollback with no prior history SHALL surface the `nothing-to-rollback` message. This view is colocated with channel configuration (list / create / set-default) in the same 渠道 tab.

#### Scenario: Promoting points the channel at the release

- **GIVEN** a published release `1.2.0` and a user holding `release:promote`
- **WHEN** the user promotes `1.2.0` to the `beta` channel
- **THEN** the `beta` channel pointer shows `1.2.0` after refetch

#### Scenario: Rollback with no history is reported

- **GIVEN** a channel that has never been promoted and a user holding `release:rollback`
- **WHEN** the user attempts rollback
- **THEN** a "nothing to rollback" message is shown
